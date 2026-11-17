//! The Tool Property palette, and the rows it shares with Sub Tool Detail.
//!
//! Every setting of a sub tool is a row with a stable name and a
//! [`Category`]. Tool Property shows the rows whose eye is open (the names
//! in [`crate::tools::SubTool::hidden`] are closed); Sub Tool Detail shows
//! every row of one category at a time, each with its eye. Both draw the
//! same functions through [`Rows`], so a setting is written once and shows
//! the same in both.

mod brush;
mod comic;
mod correct;
mod effect;
mod figure;
mod fill;
mod lines;
mod operation;
mod paint;
mod ruler;
mod select;
mod shape;
mod text;
mod three;
mod transform;

use std::ops::RangeInclusive;

use egui::{RichText, Ui};
use sue_wm::Theme;
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, TextRole};

use crate::studio::SuePaint;
use crate::tools::{SubTool, Tool};

/// Tool Property's small, muted labels and hints.
fn small_text(theme: &Theme) -> impl Fn(&str) -> RichText + '_ {
    move |text: &str| {
        RichText::new(text)
            .size(theme.text_size(TextRole::Caption))
            .color(theme.text_muted())
    }
}

/// A Blend sub tool mode that liquifies (`blend::LiquifyMode`), which the
/// Liquify button shows.
pub(super) fn is_liquify_mode(mode: u8) -> bool {
    crate::blend::is_liquify(mode)
}

/// What Sub Tool Detail sorts a setting under, in the order its list
/// shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Category {
    /// Brush Size and how the pen changes it.
    BrushSize,
    /// Opacity, hardness, color mixing: how the ink goes down.
    Ink,
    /// Anti-aliasing.
    AntiAliasing,
    /// Ready-made brush shapes to apply, and registering one.
    BrushShape,
    /// The tip's shape and how it turns.
    BrushTip,
    /// Each dab sprayed as small particles.
    SprayingEffect,
    /// Spacing, build up, flow: how dabs follow the stroke.
    Stroke,
    /// Paper grain.
    Texture,
    /// The darker, pooled rim of a watercolor stroke.
    WatercolorEdge,
    /// A second tip the stroke is laid through.
    DualBrush,
    /// Starting and ending.
    StartEnd,
    /// What an eraser erases.
    Erase,
    /// Color jitter.
    ColorJitter,
    /// Stabilization, post correction, flick, Smart Shape.
    Correction,
    /// Keeping the paint inside the reference layer's lines.
    AntiOverflow,
    /// What the tool does when it is used.
    Operation,
    /// Which layers Fill and Auto Select look at, and what they leave out.
    Reference,
    /// How a fill or a selection meets its edge.
    Area,
    /// How a new selection meets the one there.
    Selection,
    /// The Text tool's font and spacing.
    Text,
}

impl Category {
    /// Every category, in the list's order.
    pub(crate) const ALL: [Self; 20] = [
        Self::BrushSize,
        Self::Ink,
        Self::AntiAliasing,
        Self::BrushShape,
        Self::BrushTip,
        Self::SprayingEffect,
        Self::Stroke,
        Self::Texture,
        Self::WatercolorEdge,
        Self::DualBrush,
        Self::StartEnd,
        Self::Erase,
        Self::ColorJitter,
        Self::Correction,
        Self::AntiOverflow,
        Self::Operation,
        Self::Reference,
        Self::Area,
        Self::Selection,
        Self::Text,
    ];

    /// What people read.
    pub(crate) fn label(self) -> String {
        match self {
            Self::BrushSize => tr!("detail-brush-size"),
            Self::Ink => tr!("detail-ink"),
            Self::AntiAliasing => tr!("detail-anti-aliasing"),
            Self::BrushShape => tr!("detail-brush-shape"),
            Self::BrushTip => tr!("detail-brush-tip"),
            Self::SprayingEffect => tr!("detail-spraying-effect"),
            Self::Stroke => tr!("detail-stroke"),
            Self::Texture => tr!("detail-texture"),
            Self::WatercolorEdge => tr!("detail-watercolor-edge"),
            Self::DualBrush => tr!("detail-dual-brush"),
            Self::StartEnd => tr!("detail-starting-and-ending"),
            Self::Erase => tr!("detail-erase"),
            Self::ColorJitter => tr!("detail-color-jitter"),
            Self::Correction => tr!("detail-correction"),
            Self::AntiOverflow => tr!("detail-anti-overflow"),
            Self::Operation => tr!("detail-operation"),
            Self::Reference => tr!("detail-reference"),
            Self::Area => tr!("detail-area"),
            Self::Selection => tr!("detail-selection"),
            Self::Text => tr!("detail-text"),
        }
    }

    /// What the category is for, for Sub Tool Detail's help box.
    pub(crate) fn help(self) -> String {
        match self {
            Self::BrushSize => tr!("detail-help-brush-size"),
            Self::Ink => tr!("detail-help-ink"),
            Self::AntiAliasing => tr!("detail-help-anti-aliasing"),
            Self::BrushShape => tr!("detail-help-brush-shape"),
            Self::BrushTip => tr!("detail-help-brush-tip"),
            Self::SprayingEffect => tr!("detail-help-spraying-effect"),
            Self::Stroke => tr!("detail-help-stroke"),
            Self::Texture => tr!("detail-help-texture"),
            Self::WatercolorEdge => tr!("detail-help-watercolor-edge"),
            Self::DualBrush => tr!("detail-help-dual-brush"),
            Self::StartEnd => tr!("detail-help-starting-and-ending"),
            Self::Erase => tr!("detail-help-erase"),
            Self::ColorJitter => tr!("detail-help-color-jitter"),
            Self::Correction => tr!("detail-help-correction"),
            Self::AntiOverflow => tr!("detail-help-anti-overflow"),
            Self::Operation => tr!("detail-help-operation"),
            Self::Reference => tr!("detail-help-reference"),
            Self::Area => tr!("detail-help-area"),
            Self::Selection => tr!("detail-help-selection"),
            Self::Text => tr!("detail-help-text"),
        }
    }
}

/// Rows that Tool Property leaves out until a person opens their eye in
/// Sub Tool Detail (the paper texture's settings, the watercolor edge and
/// the dual brush, as Clip Studio Paint keeps them in Sub Tool Detail).
/// Their eye is kept in [`crate::tools::SubTool::hidden`] as `show:` and
/// the row's name.
const DETAIL_ONLY: &[&str] = &[
    "texture-paper",
    "texture-scale",
    "texture-rotation",
    "texture-brightness",
    "texture-contrast",
    "texture-invert",
    "texture-emphasize",
    "texture-mode",
    "texture-per-dab",
    "watercolor-edge",
    "watercolor-range",
    "watercolor-opacity",
    "watercolor-darkness",
    "watercolor-blur",
    "watercolor-after",
    "dual-brush",
    "dual-mode",
    "dual-linked",
    "dual-size",
    "dual-spacing",
    "dual-scatter",
    "dual-hardness",
    // The brush rows of FG-4 that Clip Studio Paint keeps in Sub Tool
    // Detail.
    "brush-blend",
    "blur-intensity",
    // Spraying effect's switch; a spraying brush shows its rows.
    "spray",
    "brush-shape",
    "brush-shape-apply",
    "brush-shape-register",
    "tip-kind",
    "tip-list",
    "tip-thickness",
    "tip-thickness-direction",
    "brush-density",
    "tip-flip-x",
    "tip-flip-y",
    "density-by-gap",
    "gap-preset",
    "gap-fixed",
    "continuous-spraying",
    "continuous-rate",
    "repeat-method",
    "tip-blend-darken",
    "erase-all-layers",
    "correction-by-speed",
    "correction-by-scale",
    "correction-bezier",
    "sharp-angles",
    "snapping",
    "anti-overflow",
    "anti-overflow-tolerance",
    "anti-overflow-scaling",
];

/// `true` when Tool Property leaves row `id` out, as `hidden` says.
fn left_out(hidden: &[String], id: &str) -> bool {
    if DETAIL_ONLY.contains(&id) {
        let shown = format!("show:{id}");
        !hidden.contains(&shown)
    } else {
        hidden.iter().any(|name| name == id)
    }
}

/// Opens (`show`) or closes row `id`'s eye in `hidden`.
fn set_shown(hidden: &mut Vec<String>, id: &str, show: bool) {
    if DETAIL_ONLY.contains(&id) {
        let name = format!("show:{id}");
        hidden.retain(|found| *found != name);
        if show {
            hidden.push(name);
        }
    } else {
        hidden.retain(|found| found != id);
        if !show {
            hidden.push(id.to_owned());
        }
    }
}

/// Which of the two palettes the rows are drawn in.
enum View<'h> {
    /// Tool Property: every row whose eye is open, with the category's
    /// name over each new category when `headings`.
    Property {
        hidden: &'h [String],
        headings: bool,
    },
    /// Sub Tool Detail: the rows of `category`, each with its eye.
    Detail {
        category: Category,
        hidden: &'h mut Vec<String>,
    },
    /// Nothing is drawn: the categories the rows fall in are written down
    /// (Sub Tool Detail's list).
    Census(&'h mut Vec<Category>),
}

/// The rows of a sub tool's settings as one palette draws them.
pub(crate) struct Rows<'h> {
    view: View<'h>,
    /// The settings may be changed (the sub tool is not locked).
    enabled: bool,
    /// The category of the row drawn last (for Tool Property's headings).
    last: Option<Category>,
    theme: &'h Theme,
}

impl<'h> Rows<'h> {
    /// Tool Property's rows: those not in `hidden`.
    pub(crate) fn property(
        hidden: &'h [String],
        headings: bool,
        enabled: bool,
        theme: &'h Theme,
    ) -> Self {
        Self {
            view: View::Property { hidden, headings },
            enabled,
            last: None,
            theme,
        }
    }

    /// Sub Tool Detail's rows of `category`, each with an eye that adds
    /// its name to `hidden` or takes it out.
    pub(crate) fn detail(
        category: Category,
        hidden: &'h mut Vec<String>,
        enabled: bool,
        theme: &'h Theme,
    ) -> Self {
        Self {
            view: View::Detail { category, hidden },
            enabled,
            last: None,
            theme,
        }
    }

    /// Rows that draw nothing and write down their categories.
    pub(crate) fn census(found: &'h mut Vec<Category>, theme: &'h Theme) -> Self {
        Self {
            view: View::Census(found),
            enabled: false,
            last: None,
            theme,
        }
    }

    /// `false` when this view would leave the row `id` of `category` out,
    /// so a row whose choices cost something to gather can skip them;
    /// the census counts every row.
    pub(crate) fn draws(&self, id: &str, category: Category) -> bool {
        match &self.view {
            View::Property { hidden, .. } => !left_out(hidden, id),
            View::Detail {
                category: chosen, ..
            } => *chosen == category,
            View::Census(_) => true,
        }
    }

    /// The width of a row's name, so the controls of a column line up.
    fn label_width(&self) -> f32 {
        name_width(self.theme)
    }

    /// Draws the row named `id` of `category` with `add` when this view
    /// shows it. Returns what `add` returned, or `None` when the row is
    /// not shown.
    pub(crate) fn row<R>(
        &mut self,
        ui: &mut Ui,
        id: &str,
        category: Category,
        add: impl FnOnce(&mut Ui, &Theme, bool) -> R,
    ) -> Option<R> {
        let theme = self.theme;
        let enabled = self.enabled;
        match &mut self.view {
            View::Census(found) => {
                if !found.contains(&category) {
                    found.push(category);
                }
                None
            }
            View::Property { hidden, headings } => {
                if left_out(hidden, id) {
                    return None;
                }
                if *headings && self.last != Some(category) {
                    ui.add_space(theme.space(Space::S1));
                    ui.label(
                        RichText::new(category.label())
                            .size(theme.text_size(TextRole::Caption))
                            .strong()
                            .color(theme.text),
                    );
                }
                self.last = Some(category);
                Some(add(ui, theme, enabled))
            }
            View::Detail {
                category: chosen,
                hidden,
            } => {
                if *chosen != category {
                    return None;
                }
                let shown = !left_out(hidden, id);
                let side = Density::Compact.icon_button();
                ui.horizontal(|ui| {
                    let name = if shown {
                        tr!("hide-in-tool-property")
                    } else {
                        tr!("show-in-tool-property")
                    };
                    let eye = sue_wm::ui::icon_button(
                        ("sue-paint-detail-eye", id),
                        if shown { Icon::Eye } else { Icon::EyeOff },
                        &name,
                    )
                    .selected(shown)
                    .quiet();
                    let (rect, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::hover());
                    if eye.show_at(ui, rect, theme).clicked() {
                        set_shown(hidden, id, !shown);
                    }
                    ui.vertical(|ui| add(ui, theme, enabled)).inner
                })
                .inner
                .into()
            }
        }
    }

    /// A slider row. Returns `true` when the value changed.
    #[allow(clippy::too_many_arguments, reason = "a slider and its row")]
    pub(crate) fn slider(
        &mut self,
        ui: &mut Ui,
        id: &str,
        category: Category,
        label: &str,
        value: &mut f32,
        range: RangeInclusive<f32>,
        suffix: &str,
        log: bool,
    ) -> bool {
        let width = self.label_width();
        self.row(ui, id, category, |ui, theme, enabled| {
            let mut shown = f64::from(*value);
            let decimals = if log && *range.start() < 10.0 { 1 } else { 0 };
            let mut slider = sue_wm::ui::slider(
                ("sue-paint-tool-slider", id),
                &mut shown,
                f64::from(*range.start())..=f64::from(*range.end()),
            )
            .label(label)
            .label_width(width)
            .width(field_room(ui, theme))
            .suffix(suffix)
            .decimals(decimals)
            .enabled(enabled);
            if log {
                slider = slider.logarithmic();
            }
            let changed = slider.show(ui, theme).on_hover_text(label).changed();
            if changed {
                *value = shown as f32;
            }
            changed
        })
        .unwrap_or(false)
    }

    /// [`Self::slider`] over a whole number.
    #[allow(clippy::too_many_arguments, reason = "a slider and its row")]
    pub(crate) fn slider_u32(
        &mut self,
        ui: &mut Ui,
        id: &str,
        category: Category,
        label: &str,
        value: &mut u32,
        range: RangeInclusive<u32>,
    ) -> bool {
        let mut shown = *value as f32;
        let changed = self.slider(
            ui,
            id,
            category,
            label,
            &mut shown,
            *range.start() as f32..=*range.end() as f32,
            "",
            false,
        );
        if changed {
            *value = shown.round() as u32;
        }
        changed
    }

    /// [`Self::slider`] over a share from 0 to 1, shown in percent.
    #[allow(clippy::too_many_arguments, reason = "a slider and its row")]
    pub(crate) fn percent(
        &mut self,
        ui: &mut Ui,
        id: &str,
        category: Category,
        label: &str,
        value: &mut f32,
        low: f32,
        high: f32,
    ) -> bool {
        let mut shown = *value * 100.0;
        let changed = self.slider(ui, id, category, label, &mut shown, low..=high, " %", false);
        if changed {
            *value = shown / 100.0;
        }
        changed
    }

    /// A check box row, with `hint` when the pointer rests on it. Returns
    /// `true` when it flipped.
    pub(crate) fn check(
        &mut self,
        ui: &mut Ui,
        id: &str,
        category: Category,
        label: &str,
        hint: Option<&str>,
        value: &mut bool,
    ) -> bool {
        self.row(ui, id, category, |ui, theme, enabled| {
            let response = sue_wm::ui::checkbox(("sue-paint-tool-check", id), label, *value)
                .enabled(enabled)
                .show(ui, theme);
            let response = response.on_hover_text(hint.unwrap_or(label));
            if response.clicked() {
                *value = !*value;
                return true;
            }
            false
        })
        .unwrap_or(false)
    }

    /// A choice of `options` with its name before it: side by side while
    /// they are few and fit, else a drop-down. Returns `true` when another
    /// was picked.
    pub(crate) fn choice(
        &mut self,
        ui: &mut Ui,
        id: &str,
        category: Category,
        label: &str,
        chosen: &mut usize,
        options: &[String],
    ) -> bool {
        let width = self.label_width();
        self.row(ui, id, category, |ui, theme, enabled| {
            ui.horizontal(|ui| {
                name_cell(ui, label, width, theme);
                let room = ui.available_width();
                let strip =
                    sue_wm::ui::segmented(("sue-paint-tool-choice", id), options, Some(*chosen))
                        .enabled(enabled);
                let fits = options.len() <= 4
                    && strip.size(ui.painter(), theme, Density::of(ui)).x <= room;
                if fits {
                    let answer = strip.show(ui, theme);
                    let _ = answer.response.on_hover_text(label);
                    match answer.picked {
                        Some(picked) if picked != *chosen => {
                            *chosen = picked;
                            true
                        }
                        _ => false,
                    }
                } else {
                    sue_wm::ui::input::dropdown(("sue-paint-tool-list", id), chosen, options)
                        .label(label)
                        .enabled(enabled)
                        .width(field_room(ui, theme))
                        .show(ui, theme)
                        .on_hover_text(label)
                        .changed()
                }
            })
            .inner
        })
        .unwrap_or(false)
    }

    /// [`Self::choice`] over a `u8` setting whose values are the options'
    /// places.
    pub(crate) fn choice_u8(
        &mut self,
        ui: &mut Ui,
        id: &str,
        category: Category,
        label: &str,
        value: &mut u8,
        options: &[String],
    ) -> bool {
        let mut chosen = usize::from(*value).min(options.len().saturating_sub(1));
        let changed = self.choice(ui, id, category, label, &mut chosen, options);
        if changed {
            *value = chosen as u8;
        }
        changed
    }

    /// A button row. Returns `true` when it was pressed.
    pub(crate) fn button(
        &mut self,
        ui: &mut Ui,
        id: &str,
        category: Category,
        label: &str,
    ) -> bool {
        // A button runs a command; a locked sub tool still runs it.
        self.row(ui, id, category, |ui, theme, _| {
            sue_wm::ui::button(("sue-paint-tool-button", id), label)
                .show(ui, theme)
                .on_hover_text(label)
                .clicked()
        })
        .unwrap_or(false)
    }

    /// A line of help under the settings, in Tool Property only (Sub Tool
    /// Detail has its help box).
    pub(crate) fn hint(&mut self, ui: &mut Ui, text: &str) {
        if let View::Property { .. } = self.view {
            let theme = self.theme;
            ui.label(small_text(theme)(text));
        }
    }
}

/// How wide a row's control may be, so it ends where the number fields
/// of the rows with a dynamics button end: the room left less that
/// button and the gap before it.
pub(crate) fn field_room(ui: &Ui, theme: &Theme) -> f32 {
    let side = Density::of(ui).icon_button();
    (ui.available_width() - side - theme.space(Space::S2)).max(side)
}

/// How wide the names of a palette's rows are, so their controls line up.
pub(crate) fn name_width(theme: &Theme) -> f32 {
    theme.space(Space::S9) * 2.0
}

/// A row's name in the column the names share, `width` wide.
pub(crate) fn name_cell(ui: &mut Ui, label: &str, width: f32, theme: &Theme) {
    ui.add_sized(
        egui::vec2(width, Density::of(ui).control_height()),
        egui::Label::new(
            RichText::new(label)
                .size(theme.text_size(TextRole::Caption))
                .color(theme.text_muted()),
        )
        .truncate(),
    );
}

/// A Blending mode row over `blend`: how the tool's color mixes with
/// the pixels under it. Returns `true` when another was picked.
pub(crate) fn blend_mode_row(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    id: &str,
    blend: &mut sue_model::BlendMode,
) -> bool {
    let modes: Vec<String> = sue_model::BlendMode::ALL
        .iter()
        .map(|mode| sue_lang::translate("model", mode.label()))
        .collect();
    let mut chosen = sue_model::BlendMode::ALL
        .iter()
        .position(|mode| mode == blend)
        .unwrap_or(0);
    let changed = rows.choice(
        ui,
        id,
        Category::Ink,
        &tr!("blending-mode"),
        &mut chosen,
        &modes,
    );
    if changed {
        *blend = sue_model::BlendMode::ALL[chosen];
    }
    changed
}

/// The names of `options` in the language in use.
pub(crate) fn names(options: &[&str]) -> Vec<String> {
    options
        .iter()
        .map(|option| crate::localize(option))
        .collect()
}

impl SuePaint {
    /// The settings of the current sub tool through `rows`, and a command
    /// they ask for. Tool Property draws them, Sub Tool Detail draws one
    /// category of them, and its list counts their categories.
    pub(crate) fn sub_tool_rows(
        &mut self,
        ui: &mut Ui,
        rows: &mut Rows<'_>,
        tool: Tool,
        theme: &Theme,
    ) -> Option<&'static str> {
        let mut command = None;
        let sub = self.toolbox.current_mut();
        match tool {
            tool if tool.paints() => {
                // Vector magnet is a correction of the line: in Sub Tool
                // Detail it shows with Correction, with its eye.
                if matches!(tool, Tool::Pen | Tool::Pencil | Tool::Brush | Tool::Figure) {
                    rows.row(ui, "vector-magnet", Category::Correction, |ui, _, _| {
                        self.magnet_row(ui, theme);
                    });
                }
                let sub = self.toolbox.current_mut();
                command = paint::paint_properties(ui, rows, tool, sub, &mut self.brush_work);
                if tool == Tool::Figure {
                    figure::figure_properties(ui, rows, sub);
                }
            }
            Tool::Fill | Tool::AutoSelect => {
                fill::fill_properties(ui, rows, tool, sub, &mut self.select_op);
                if tool == Tool::Fill {
                    self.vector_path_row(ui, theme);
                }
            }
            Tool::Gradient => {
                let colors = [self.main, self.sub, self.drawing_color()];
                let parts = fill::GradientParts {
                    library: &mut self.gradients,
                    stop: &mut self.gradient_work.stop,
                    colors,
                };
                fill::gradient_properties(ui, rows, self.toolbox.current_mut(), parts);
            }
            Tool::Select => select::select_properties(ui, rows, sub, &mut self.select_op),
            Tool::Eyedropper => operation::eyedropper_properties(ui, rows, sub),
            Tool::Zoom => operation::zoom_properties(ui, rows, sub),
            Tool::Hand => hand_properties(ui, rows, sub),
            Tool::MoveLayer => operation::move_properties(ui, rows, sub),
            Tool::Ruler => command = ruler::ruler_properties(ui, rows, sub),
            Tool::CorrectLine => correct::correct_line_properties(ui, rows, sub),
            Tool::Blend => paint::blend_properties(ui, rows, sub),
            Tool::Frame => comic::frame_properties(ui, rows, sub),
            Tool::Balloon => comic::balloon_properties(ui, rows, sub),
            Tool::Text => text::text_properties(ui, rows, sub, self.typing.as_mut(), theme),
            // The Operation tool's four sub tools.
            Tool::Operation => operation::operation_properties(ui, rows, sub),
            _ => rows.hint(ui, &tr!("tool-arrives-later-sue-paint")),
        }
        command
    }

    /// The gradient layer Tool Property edits instead of the sub tool:
    /// the current layer, when it is one and the Gradient tool (but for
    /// Contour line fill) or the Object tool is on.
    pub(crate) fn edited_gradient_layer(
        &self,
        arena: &sue_core::Arena,
    ) -> Option<sue_core::EntityId> {
        let tool = self.toolbox.tool();
        let edits = tool == Tool::Operation
            || (tool == Tool::Gradient
                && self.toolbox.current().mode != crate::tools::CONTOUR_FILL);
        if !edits {
            return None;
        }
        self.gradient_layer(arena).map(|(layer, _)| layer)
    }

    /// The settings of the current sub tool, and a command they ask for:
    /// Tool Property's body.
    pub(super) fn tool_properties(
        &mut self,
        ui: &mut Ui,
        tool: Tool,
        arena: &sue_core::Arena,
        theme: &Theme,
    ) -> Option<&'static str> {
        // A transform open: its settings, whatever the tool (not Move
        // layer's drag of selected pixels, which lifts them the same way).
        if self.transforming() {
            self.transform_properties(ui, theme);
            return None;
        }
        if tool == Tool::Operation
            && let Some(folder) = self.camera_folder(arena)
        {
            return self.camera_properties(ui, folder, arena, theme);
        }
        if tool == Tool::Operation
            && let Some(layer) = self
                .layer()
                .filter(|layer| crate::three::is_3d(arena, *layer))
        {
            self.view3d_properties(ui, layer, arena, theme);
            return None;
        }
        if tool == Tool::Operation
            && let Some(layer) = self.layer()
            && let Some(lines) = Self::effect_of(arena, layer)
        {
            self.effect_properties(ui, layer, lines, theme);
            return None;
        }
        // The Object tool on a vector layer: the picked lines' settings.
        if tool == Tool::Operation && self.line_properties(ui, arena, theme) {
            return None;
        }
        // The Gradient or Object tool on a gradient layer: its gradient,
        // which stays editable.
        if self.edited_gradient_layer(arena).is_some()
            && let Some((layer, mut gradient)) = self.gradient_layer(arena)
        {
            ui.spacing_mut().item_spacing.y = theme.space(Space::S3);
            let colors = [self.main, self.sub, self.drawing_color()];
            // A locked layer shows its gradient but takes no change.
            let unlocked = self.layer_info(arena).is_none_or(|info| !info.locked);
            let mut rows = Rows::property(&[], self.tool_palettes.headings, unlocked, theme);
            if fill::gradient_layer_properties(
                ui,
                &mut rows,
                &mut gradient,
                &mut self.gradient_work.stop,
                colors,
                &mut self.gradients,
            ) && unlocked
            {
                self.gradient_work.pending = Some((layer, gradient));
            }
            return None;
        }
        ui.spacing_mut().item_spacing.y = theme.space(Space::S3);
        let sub = self.toolbox.current();
        let hidden = sub.hidden.clone();
        let enabled = !sub.locked;
        let mut rows = Rows::property(&hidden, self.tool_palettes.headings, enabled, theme);
        self.sub_tool_rows(ui, &mut rows, tool, theme)
    }
}

/// The Hand tool's sub tools: Hand scrolls, Rotate turns the view.
///
/// Rotate's Snap angle (`SubTool::angle_step`, 0 for off): while it is
/// on the view turns in its steps, and Shift snaps by the same step
/// (CC-1).
fn hand_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    rows.choice_u8(
        ui,
        "hand-mode",
        Category::Operation,
        &tr!("mode"),
        &mut sub.mode,
        &[tr!("scroll"), tr!("rotate")],
    );
    if sub.mode != 1 {
        return;
    }
    let mut snap = sub.angle_step > 0.0;
    if rows.check(
        ui,
        "hand-snap-angle",
        Category::Operation,
        &tr!("snap-angle"),
        Some(&tr!("snap-angle-hint")),
        &mut snap,
    ) {
        sub.angle_step = if snap { 45.0 } else { 0.0 };
    }
    if snap {
        rows.slider(
            ui,
            "hand-angle-step",
            Category::Operation,
            &tr!("step-of-angle"),
            &mut sub.angle_step,
            1.0..=90.0,
            &tr!("degrees-short"),
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_has_a_name_and_help_in_english() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let mut names = std::collections::BTreeSet::new();
        for category in Category::ALL {
            assert!(!category.label().is_empty(), "{category:?}");
            assert!(
                category.help().len() > category.label().len(),
                "{category:?}"
            );
            assert!(names.insert(category.label()), "{category:?} named twice");
        }
        // The list's order is the enum's.
        let mut sorted = Category::ALL;
        sorted.sort();
        assert_eq!(sorted, Category::ALL);
    }
}
