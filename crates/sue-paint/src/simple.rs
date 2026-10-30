//! Simple Mode, Sue Paint's touch layout for drawing and little else: the
//! canvas fills the page; Sue's top bar holds a back arrow, Undo and Redo,
//! the reference pictures and a "..." menu; brush size and opacity sliders
//! with their numbers sit near the bottom; and a bottom bar swaps brush and
//! eraser, picks a pen, a brush, the color and the layers. Studio Mode
//! ([`crate::studio_mode`]) is the fuller layout; the choice is
//! `sue_paint.touch_mode` in the rice.

use egui::{Color32, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
use sue_wm::start::{self as page, Glyph};
use sue_wm::{AppletContext, Theme};

use crate::icons::Icon;
use crate::panels::{icon_button, tool_icon};
use crate::phone::Sheet;
use crate::studio::SuePaint;
use crate::tools::Tool;

/// The rice key of the touch layout: `simple` or `studio`.
pub const MODE_KEY: &str = "sue_paint.touch_mode";

/// The tools Simple Mode's pen button offers.
pub(crate) const PENS: [Tool; 9] = [
    Tool::Pen,
    Tool::Pencil,
    Tool::Brush,
    Tool::Airbrush,
    Tool::Decoration,
    Tool::Eraser,
    Tool::Blend,
    Tool::Fill,
    Tool::Gradient,
];

/// The bottom bar's height.
const BAR: f32 = 64.0;
/// A thumb-sized button.
const BUTTON: f32 = 44.0;

/// `true` when the touch layout is Studio Mode.
pub(crate) fn studio_mode(cx: &AppletContext<'_>) -> bool {
    cx.rice().get_str(MODE_KEY) == Some("studio")
}

/// Switches the touch layout (`studio` or `simple`), through the rice so
/// Settings shows it too.
pub(crate) fn set_mode(cx: &mut AppletContext<'_>, studio: bool) {
    let value = if studio { "studio" } else { "simple" };
    if let Err(error) = cx.edit_rice(sue_rice::SetRiceValue::new(
        MODE_KEY,
        sue_rice::RiceValue::Text(value.to_owned()),
    )) {
        cx.fail(error.to_string());
    }
}

impl SuePaint {
    /// Simple Mode's row in Sue's top bar: back to the start page at the
    /// left; Undo, Redo, the reference pictures and "..." at the right.
    pub(crate) fn simple_bar(
        &mut self,
        ui: &mut Ui,
        bar: Rect,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) -> Option<String> {
        let mut run = None;
        let side = BUTTON.min(bar.height());
        let at =
            |x: f32| Rect::from_min_size(pos2(x, bar.center().y - side / 2.0), Vec2::splat(side));
        if page::icon_button(
            ui,
            at(bar.left() + 2.0),
            Glyph::Back,
            &tr!("start-back"),
            false,
            theme,
        )
        .clicked()
        {
            self.start.forced = true;
            self.sheet = None;
        }
        let right = |index: usize| bar.right() - 2.0 - (index as f32 + 1.0) * (side + 2.0);
        if page::icon_button(
            ui,
            at(right(0)),
            Glyph::More,
            &tr!("simple-more"),
            self.sheet == Some(Sheet::More),
            theme,
        )
        .clicked()
        {
            self.toggle_sheet(Sheet::More);
        }
        if icon_button(
            ui,
            at(right(1)),
            Icon::Reference,
            true,
            self.sheet == Some(Sheet::Reference),
            &tr!("simple-reference"),
            theme,
        ) {
            self.toggle_sheet(Sheet::Reference);
        }
        if icon_button(
            ui,
            at(right(2)),
            Icon::Redo,
            cx.history.can_redo(),
            false,
            &tr!("redo-three-finger-tap"),
            theme,
        ) {
            run = Some("edit.redo".to_owned());
        }
        if icon_button(
            ui,
            at(right(3)),
            Icon::Undo,
            cx.history.can_undo(),
            false,
            &tr!("undo-two-finger-tap"),
            theme,
        ) {
            run = Some("edit.undo".to_owned());
        }
        run
    }

    /// Simple Mode's page: the canvas, the size and opacity sliders, and
    /// the bottom bar.
    pub(crate) fn simple_frame(
        &mut self,
        ui: &mut Ui,
        full: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let bar = Rect::from_min_max(pos2(full.left(), full.bottom() - BAR), full.max);
        let middle = Rect::from_min_max(full.min, pos2(full.right(), bar.top()));
        self.canvas_area(ui, middle, cx, theme);
        let mut run: Option<String> = None;

        // The sliders, over the bottom of the canvas.
        if self.sheet.is_none() {
            // At most 560 points wide, in the middle on a tablet.
            let width = (middle.width() - 24.0).min(560.0);
            let sliders = Rect::from_center_size(
                pos2(middle.center().x, middle.bottom() - 46.0),
                vec2(width, 76.0),
            );
            ui.painter()
                .rect_filled(sliders, 0.0, theme.panel.gamma_multiply(0.85));
            let mut inner = ui.new_child(
                UiBuilder::new()
                    .max_rect(sliders.shrink2(vec2(10.0, 6.0)))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            let width = sliders.width() - 20.0;
            inner.spacing_mut().slider_width = (width - 90.0).max(60.0);
            inner.spacing_mut().interact_size.y = 30.0;
            let tip = &mut self.toolbox.current_mut().tip;
            inner.horizontal(|ui| {
                ui.add(
                    egui::Slider::new(&mut tip.size, 0.5..=1000.0)
                        .logarithmic(true)
                        .show_value(false),
                )
                .on_hover_text(tr!("size-2"));
                ui.add(
                    egui::DragValue::new(&mut tip.size)
                        .range(0.5..=1000.0)
                        .speed(0.5)
                        .max_decimals(1),
                );
            });
            let mut opacity = tip.opacity * 100.0;
            inner.horizontal(|ui| {
                let changed = ui
                    .add(egui::Slider::new(&mut opacity, 0.0..=100.0).show_value(false))
                    .changed()
                    | ui.add(
                        egui::DragValue::new(&mut opacity)
                            .range(0.0..=100.0)
                            .max_decimals(0),
                    )
                    .changed();
                if changed {
                    tip.opacity = opacity / 100.0;
                }
            });
        }

        // The bottom bar: swap brush and eraser, pens, brushes, the color
        // and the layers.
        ui.painter().rect_filled(bar, 0.0, theme.panel);
        ui.painter()
            .hline(bar.x_range(), bar.top(), theme.hairline());
        let cell = bar.width() / 5.0;
        let slot = |index: usize| {
            Rect::from_center_size(
                pos2(bar.left() + cell * (index as f32 + 0.5), bar.center().y),
                Vec2::splat(BUTTON),
            )
        };
        let tool = self.toolbox.tool();
        if icon_button(
            ui,
            slot(0),
            Icon::Swap,
            true,
            false,
            &tr!("simple-swap"),
            theme,
        ) {
            if tool == Tool::Eraser {
                self.toolbox.pick(self.drawing_tool);
            } else {
                self.drawing_tool = tool;
                self.toolbox.pick(Tool::Eraser);
            }
        }
        if icon_button(
            ui,
            slot(1),
            tool_icon(tool),
            true,
            self.sheet == Some(Sheet::Pens),
            &tr!("simple-pens"),
            theme,
        ) {
            self.toggle_sheet(Sheet::Pens);
        }
        if icon_button(
            ui,
            slot(2),
            Icon::Brush,
            true,
            self.sheet == Some(Sheet::ToolSettings),
            &tr!("simple-brushes"),
            theme,
        ) {
            self.toggle_sheet(Sheet::ToolSettings);
        }
        let color = slot(3);
        let [r, g, b] = self.main;
        ui.painter()
            .circle_filled(color.center(), 16.0, Color32::from_rgb(r, g, b));
        ui.painter()
            .circle_stroke(color.center(), 17.0, theme.hairline_active());
        if ui
            .interact(
                color,
                ui.scope_id().with("sue-paint-simple-color"),
                Sense::click(),
            )
            .on_hover_text(tr!("color-2"))
            .clicked()
        {
            self.toggle_sheet(Sheet::Color);
        }
        if icon_button(
            ui,
            slot(4),
            Icon::Layers,
            true,
            self.sheet == Some(Sheet::Layers),
            &tr!("layers-3"),
            theme,
        ) {
            self.toggle_sheet(Sheet::Layers);
        }

        if let Some(sheet) = self.sheet {
            self.phone_sheet(ui, middle, sheet, cx, theme, &mut run);
        }
        if let Some(command) = run {
            self.run(&command, cx);
        }
    }

    /// The "..." menu of Simple Mode, as rows. Returns the command a row
    /// runs.
    pub(crate) fn simple_more(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Option<String> {
        use sue_wm::start::{Row, Trailing};
        let rows = [
            (
                Row::new(
                    Some(Glyph::External),
                    tr!("simple-export-image"),
                    Trailing::None,
                ),
                "file.export.png",
            ),
            (
                Row::new(
                    Some(Glyph::Film),
                    tr!("export-timelapse-ellipsis"),
                    Trailing::None,
                ),
                "file.timelapse.export",
            ),
            (
                Row::new(
                    Some(Glyph::Transfers),
                    tr!("simple-flip-view"),
                    Trailing::None,
                ),
                "view.flip_h",
            ),
            (
                Row::new(
                    Some(Glyph::Picture),
                    tr!("simple-see-references"),
                    Trailing::More,
                ),
                "sheet.reference",
            ),
            (
                Row::new(
                    Some(Glyph::Film),
                    tr!("simple-create-animation"),
                    Trailing::None,
                ),
                "anim.timeline.new",
            ),
            (
                Row::new(
                    Some(Glyph::Settings),
                    tr!("simple-canvas-setting"),
                    Trailing::None,
                ),
                "edit.canvas_properties",
            ),
            (
                Row::new(
                    Some(Glyph::Wrench),
                    tr!("preferences-ellipsis"),
                    Trailing::None,
                ),
                "file.preferences",
            ),
            (
                Row::new(Some(Glyph::Grid), tr!("simple-to-studio"), Trailing::None),
                "mode.studio",
            ),
            (
                Row::new(Some(Glyph::Help), tr!("start-help"), Trailing::External),
                "help.guide",
            ),
        ];
        let shown: Vec<Row> = rows.iter().map(|(row, _)| row.clone()).collect();
        let chosen = page::group(ui, &shown, theme)?;
        match rows[chosen].1 {
            "sheet.reference" => {
                self.sheet = Some(Sheet::Reference);
                None
            }
            "mode.studio" => {
                set_mode(cx, true);
                self.sheet = None;
                None
            }
            command => {
                self.sheet = None;
                Some(command.to_owned())
            }
        }
    }
}
