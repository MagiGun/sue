//! Simple Mode, Sue Paint's touch layout for drawing and little else: the
//! canvas fills the page; Sue's top bar holds a back arrow, Undo and Redo,
//! Draw with Fingers, the reference pictures and a "..." menu; brush size
//! and opacity sliders with their numbers sit near the bottom; and a
//! bottom bar swaps brush and eraser, picks a pen, a brush, the color and
//! the layers. Studio Mode ([`crate::studio_mode`]) is the fuller layout;
//! the choice is `sue_paint.touch_mode` in the rice.

use egui::{CornerRadius, Rect, Ui, UiBuilder, Vec2, pos2};
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Density, Space};
use sue_wm::{AppletContext, Facts, Theme};

use crate::phone::Sheet;
use crate::studio::SuePaint;
use crate::studio_mode::tool_glyph;
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

/// The commands at the right of Simple Mode's top bar, from the right
/// edge in, after "..." and the reference pictures.
const TOP_BAR: [&str; 3] = ["edit.redo", "edit.undo", "touch.finger_draws"];

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

/// A row of Simple Mode's "..." menu: its icon, its name, and what it
/// does (a command, or one of the rows' own words below).
type MoreRow = (Icon, String, &'static str);

impl SuePaint {
    /// Simple Mode's row in Sue's top bar: back to the Hub at the
    /// left; Draw with Fingers, Undo, Redo, the reference pictures and
    /// "..." at the right.
    pub(crate) fn simple_bar(
        &mut self,
        ui: &mut Ui,
        bar: Rect,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) -> Option<String> {
        let mut run = None;
        let side = Density::Touch.icon_button().min(bar.height());
        let gap = theme.space(Space::S1);
        let at =
            |x: f32| Rect::from_min_size(pos2(x, bar.center().y - side / 2.0), Vec2::splat(side));
        if ui::icon_button("sue-paint-simple-back", Icon::ArrowLeft, &tr!("start-back"))
            .show_at(ui, at(bar.left() + gap), theme)
            .clicked()
        {
            run = Some("file.start_page".to_owned());
            self.sheet = None;
        }
        let right = |index: usize| bar.right() - (index as f32 + 1.0) * (side + gap);
        if ui::icon_button("sue-paint-simple-more", Icon::More, &tr!("simple-more"))
            .selected(self.sheet == Some(Sheet::More))
            .show_at(ui, at(right(0)), theme)
            .clicked()
        {
            self.toggle_sheet(Sheet::More);
        }
        if ui::icon_button(
            "sue-paint-simple-reference",
            Icon::PaletteSubView,
            &tr!("simple-reference"),
        )
        .selected(self.sheet == Some(Sheet::Reference))
        .show_at(ui, at(right(1)), theme)
        .clicked()
        {
            self.toggle_sheet(Sheet::Reference);
        }
        let facts = Facts::of(cx);
        for (index, command) in TOP_BAR.iter().enumerate() {
            if self.touch_command_button(ui, at(right(index + 2)), command, &facts, theme) {
                run = Some((*command).to_owned());
            }
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
        let button = Density::Touch.icon_button();
        let pad = theme.space(Space::S5);
        let bar = Rect::from_min_max(pos2(full.left(), full.bottom() - button - pad), full.max);
        let middle = Rect::from_min_max(full.min, pos2(full.right(), bar.top()));
        self.canvas_area(ui, middle, cx, theme);
        let mut run: Option<String> = None;

        // The sliders, over the bottom of the canvas.
        if self.sheet.is_none() {
            self.simple_sliders(ui, middle, theme);
        }

        // The bottom bar: swap brush and eraser, pens, brushes, the color
        // and the layers.
        ui.painter()
            .rect_filled(bar, CornerRadius::ZERO, theme.panel);
        sue_wm::cover::cover(ui.ctx(), ui.layer_id(), bar);
        ui.painter()
            .hline(bar.x_range(), bar.top(), theme.hairline());
        let cell = bar.width() / 5.0;
        let slot = |index: usize| {
            Rect::from_center_size(
                pos2(bar.left() + cell * (index as f32 + 0.5), bar.center().y),
                Vec2::splat(button),
            )
        };
        let tool = self.toolbox.tool();
        if ui::icon_button("sue-paint-simple-swap", Icon::Swap, &tr!("simple-swap"))
            .show_at(ui, slot(0), theme)
            .clicked()
        {
            self.swap_eraser();
        }
        if ui::icon_button(
            "sue-paint-simple-pens",
            tool_glyph(tool),
            &tr!("simple-pens"),
        )
        .selected(self.sheet == Some(Sheet::Pens))
        .show_at(ui, slot(1), theme)
        .clicked()
        {
            self.toggle_sheet(Sheet::Pens);
        }
        if ui::icon_button(
            "sue-paint-simple-brushes",
            Icon::PaletteSubTools,
            &tr!("simple-brushes"),
        )
        .selected(self.sheet == Some(Sheet::ToolSettings))
        .show_at(ui, slot(2), theme)
        .clicked()
        {
            self.toggle_sheet(Sheet::ToolSettings);
        }
        self.color_swatch(ui, slot(3), theme);
        if ui::icon_button("sue-paint-simple-layers", Icon::Layers, &tr!("layers-3"))
            .selected(self.sheet == Some(Sheet::Layers))
            .show_at(ui, slot(4), theme)
            .clicked()
        {
            self.toggle_sheet(Sheet::Layers);
        }

        if let Some(sheet) = self.sheet {
            self.phone_sheet(ui, middle, sheet, cx, theme, &mut run);
        }
        if let Some(command) = run {
            self.run(&command, cx);
        }
    }

    /// Brush size and opacity, each a slider with its number, on a card
    /// over the bottom of `middle`: at most a phone wide, in the middle
    /// on a tablet.
    fn simple_sliders(&mut self, ui: &mut Ui, middle: Rect, theme: &Theme) {
        let density = Density::Touch;
        let pad = theme.space(Space::S5);
        let (gap, inset) = (theme.space(Space::S2), theme.space(Space::S4));
        let width = (middle.width() - pad * 2.0).min(ui::Tile::default_size(density).x * 3.0);
        let height = density.control_height() * 2.0 + gap + inset * 2.0;
        let card = Rect::from_center_size(
            pos2(middle.center().x, middle.bottom() - pad - height / 2.0),
            Vec2::new(width, height),
        );
        ui.painter()
            .rect_filled(card, theme.corner(ui::Radius::Lg), theme.surface(1));
        ui.painter().rect_stroke(
            card,
            theme.corner(ui::Radius::Lg),
            theme.hairline(),
            egui::StrokeKind::Inside,
        );
        sue_wm::cover::cover(ui.ctx(), ui.layer_id(), card);
        let mut inner = ui.new_child(
            UiBuilder::new()
                .max_rect(card.shrink(inset))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        inner.spacing_mut().item_spacing.y = gap;
        let tip = &mut self.toolbox.current_mut().tip;
        let mut size = f64::from(tip.size);
        let mut opacity = f64::from(tip.opacity) * 100.0;
        let (size_name, opacity_name) = (tr!("size-2"), tr!("opacity"));
        let px = tr!("px");
        ui::slider("sue-paint-simple-size", &mut size, 0.5..=1000.0)
            .label(&size_name)
            .logarithmic()
            .decimals(1)
            .field(true)
            .suffix(&px)
            .show(&mut inner, theme);
        ui::slider("sue-paint-simple-opacity", &mut opacity, 0.0..=100.0)
            .label(&opacity_name)
            .field(true)
            .suffix("%")
            .show(&mut inner, theme);
        tip.size = size as f32;
        tip.opacity = (opacity / 100.0) as f32;
    }

    /// The "..." menu of Simple Mode, as rows. Returns the command a row
    /// runs.
    pub(crate) fn simple_more(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Option<String> {
        let facts = Facts::of(cx);
        let recording = self
            .state("file.timelapse.record", &facts)
            .checked
            .unwrap_or(false);
        let rows: [MoreRow; 11] = [
            (Icon::Export, tr!("simple-export-image"), "file.export.png"),
            (
                Icon::Film,
                tr!("export-timelapse-ellipsis"),
                "file.timelapse.export",
            ),
            (
                Icon::History,
                crate::commands::registry()
                    .label("file.timelapse.record")
                    .unwrap_or_default(),
                "file.timelapse.record",
            ),
            (Icon::FlipHorizontal, tr!("simple-flip-view"), "view.flip_h"),
            (
                Icon::PaletteSubView,
                tr!("simple-see-references"),
                "sheet.reference",
            ),
            (
                Icon::TimelineNew,
                tr!("simple-create-animation"),
                "anim.timeline.new",
            ),
            (
                Icon::Info,
                tr!("simple-canvas-setting"),
                "edit.canvas_properties",
            ),
            (
                Icon::Settings,
                tr!("preferences-ellipsis"),
                "file.preferences",
            ),
            (Icon::Finger, tr!("draw-with-fingers"), "touch.finger_draws"),
            (Icon::LayoutStyle, tr!("simple-to-studio"), "mode.studio"),
            (Icon::Help, tr!("start-help"), "help.guide"),
        ];
        let mut chosen = None;
        for (icon, title, what) in &rows {
            let checked = match *what {
                "file.timelapse.record" => Some(recording),
                "touch.finger_draws" => self.state(what, &facts).checked,
                _ => None,
            };
            // A switch says whether it is on; a command, its keys.
            let note = match checked {
                Some(true) => tr!("on"),
                Some(false) => tr!("off"),
                None => self.keys_of(what),
            };
            let row = ui::list_row(("sue-paint-simple-more", *what), title)
                .icon(*icon)
                .note(&note);
            if row.show(ui, theme).response.clicked() {
                chosen = Some(*what);
            }
        }
        match chosen? {
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
