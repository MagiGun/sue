//! The brush rows of the full guide's FG-4: the brush's own blending
//! mode and how a mixing brush carries its paint (Ink), brush shape
//! presets, a tip of several pictures with its thickness, direction,
//! flips and density (Brush Tip), the spraying effect, the gap presets,
//! continuous spraying and the repeat method (Stroke), Erase all layers,
//! Sharp angles, post correction by speed, by scale and as Bezier
//! curves, snapping (Correction) and anti-overflow.
//!
//! Each row changes the sub tool's [`crate::tools::brush_rows::BrushRows`]
//! or its paint behaviours (`SubTool::behaviours`), which the brush
//! engine paints with and the brush files keep (`crate::brush_rows`).

use egui::{RichText, Ui};
use sue_brush::paint::{Behaviours, Flip, ParticleDirection, Spray, TipBlend, TipOrder};
use sue_wm::icons::Icon;
use sue_wm::ui::{Space, TextRole};

use super::{Category, Rows};
use crate::brush_rows::{BrushWork, preset_rows_of, tips_of};
use crate::tools::brush_rows::{AntiOverflow, Gap, PresetRows};
use crate::tools::{SubTool, Tool};

/// Changes `sub`'s preset rows with `change`, which says whether it
/// changed them; they are kept only when it did.
fn edit_rows(sub: &mut SubTool, change: impl FnOnce(&mut PresetRows) -> bool) {
    let mut rows = preset_rows_of(sub);
    if change(&mut rows) {
        sub.brush_rows.preset = Some(rows);
    }
}

/// Changes `sub`'s paint behaviours with `change`; they are kept only
/// when they changed.
fn edit_behaviours(sub: &mut SubTool, change: impl FnOnce(&mut Behaviours)) {
    let before = crate::canvas::engine::behaviours_of(sub);
    let mut b = before.clone();
    change(&mut b);
    if b != before {
        sub.behaviours = Some(b);
    }
}

/// The brush's blending modes as the Ink row lists them: the layer
/// blending modes, then Behind (the paint goes under what the layer
/// holds).
fn blend_names() -> Vec<String> {
    let mut names: Vec<String> = sue_model::BlendMode::ALL
        .iter()
        .map(|mode| sue_lang::translate("model", mode.label()))
        .collect();
    names.push(tr!("blend-behind"));
    names
}

/// Ink: the brush's own blending mode (00:28:16), and for a mixing brush
/// its amount and density of paint, color stretch and blur intensity
/// (00:28:27, 00:28:47).
pub(super) fn ink_rows(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    if sub.tip.eraser {
        return;
    }
    let names = blend_names();
    let behind = crate::canvas::engine::behaviours_of(sub).behind;
    let current = preset_rows_of(sub);
    let mut chosen = if behind {
        names.len() - 1
    } else {
        sue_model::BlendMode::ALL
            .iter()
            .position(|mode| *mode == current.blend)
            .unwrap_or(0)
    };
    if rows.choice(
        ui,
        "brush-blend",
        Category::Ink,
        &tr!("blending-mode"),
        &mut chosen,
        &names,
    ) {
        let behind = chosen + 1 == names.len();
        edit_behaviours(sub, |b| b.behind = behind);
        edit_rows(sub, |r| {
            r.blend = if behind {
                sue_model::BlendMode::Normal
            } else {
                sue_model::BlendMode::ALL[chosen]
            };
            true
        });
    }
    if sub.tip.mix <= 0.0 {
        return;
    }
    let mut r = preset_rows_of(sub);
    let mut changed = rows.percent(
        ui,
        "paint-amount",
        Category::Ink,
        &tr!("amount-of-paint"),
        &mut r.amount,
        0.0,
        100.0,
    );
    let mut density = crate::canvas::engine::behaviours_of(sub).paint_density;
    if rows.percent(
        ui,
        "paint-density",
        Category::Ink,
        &tr!("density-of-paint"),
        &mut density,
        0.0,
        100.0,
    ) {
        edit_behaviours(sub, |b| b.paint_density = density.clamp(0.0, 1.0));
    }
    changed |= rows.percent(
        ui,
        "color-stretch",
        Category::Ink,
        &tr!("color-stretch"),
        &mut r.stretch,
        0.0,
        100.0,
    );
    changed |= rows.percent(
        ui,
        "blur-intensity",
        Category::Ink,
        &tr!("blur-intensity"),
        &mut r.blur,
        0.0,
        100.0,
    );
    if changed {
        sub.brush_rows.preset = Some(r);
    }
}

/// Brush Shape (00:29:47): a list of ready-made shapes, Apply to lay the
/// one picked on this sub tool, and Register to keep this sub tool's own.
/// Returns the command Register asks for.
pub(super) fn shape_rows(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    sub: &mut SubTool,
    work: &mut BrushWork,
) -> Option<&'static str> {
    let presets = crate::brush_rows::shape_presets(&work.kept);
    let names: Vec<String> = presets
        .iter()
        .map(|preset| crate::localize(&preset.name))
        .collect();
    let mut chosen = work.shape.min(names.len().saturating_sub(1));
    if rows.choice(
        ui,
        "brush-shape",
        Category::BrushShape,
        &tr!("brush-shape"),
        &mut chosen,
        &names,
    ) {
        work.shape = chosen;
    }
    if rows.button(
        ui,
        "brush-shape-apply",
        Category::BrushShape,
        &tr!("apply-brush-shape"),
    ) && !sub.locked
        && let Some(from) = presets.get(work.shape)
    {
        crate::brush_rows::copy_shape(from, sub);
    }
    rows.button(
        ui,
        "brush-shape-register",
        Category::BrushShape,
        &tr!("register-brush-shape"),
    )
    .then_some("sub_tool.register_shape")
}

/// What people read for a flip setting.
fn flip_names() -> Vec<String> {
    vec![tr!("flip-off"), tr!("flip-on"), tr!("flip-random")]
}

/// Brush Tip (00:29:53, 00:30:24, 00:30:34, FGV-24): round or pictures,
/// the list of pictures with a picker that searches their names and tags
/// (00:45:55), thickness and its direction, flips, the brush density and
/// adjusting it by the gap.
pub(super) fn tip_rows(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool, work: &mut BrushWork) {
    let tips = tips_of(sub);
    let mut kind = usize::from(!tips.is_empty());
    if rows.choice(
        ui,
        "tip-kind",
        Category::BrushTip,
        &tr!("tip-shape"),
        &mut kind,
        &[tr!("tip-circle"), tr!("tip-material")],
    ) {
        if kind == 0 {
            sub.tip.shape = 0;
            sub.tip.shape_count = 1;
            edit_rows(sub, |r| {
                r.tips.clear();
                true
            });
            work.tip_search = None;
        } else {
            work.tip_search = Some(String::new());
        }
    }
    if kind == 1 || work.tip_search.is_some() {
        tip_list(ui, rows, sub, work, &tips);
    }
    let mut r = preset_rows_of(sub);
    let mut changed = rows.percent(
        ui,
        "tip-thickness",
        Category::BrushTip,
        &tr!("thickness"),
        &mut r.thickness,
        1.0,
        100.0,
    );
    r.thickness = r.thickness.clamp(0.01, 1.0);
    let mut direction = usize::from(r.vertical);
    if rows.choice(
        ui,
        "tip-thickness-direction",
        Category::BrushTip,
        &tr!("direction-of-thickness"),
        &mut direction,
        &[tr!("thickness-horizontal"), tr!("thickness-vertical")],
    ) {
        r.vertical = direction == 1;
        changed = true;
    }
    changed |= rows.percent(
        ui,
        "brush-density",
        Category::BrushTip,
        &tr!("brush-density"),
        &mut r.density,
        0.0,
        100.0,
    );
    if changed {
        sub.brush_rows.preset = Some(r);
    }
    let b = crate::canvas::engine::behaviours_of(sub);
    let flips = flip_names();
    for (axis, id, label) in [
        (0, "tip-flip-x", tr!("flip-horizontal")),
        (1, "tip-flip-y", tr!("flip-vertical")),
    ] {
        let mut at = Flip::ALL
            .iter()
            .position(|flip| *flip == b.flip[axis])
            .unwrap_or(0);
        if rows.choice(ui, id, Category::BrushTip, &label, &mut at, &flips) {
            edit_behaviours(sub, |b| b.flip[axis] = Flip::ALL[at]);
        }
    }
    let mut by_gap = b.gap_density;
    if rows.check(
        ui,
        "density-by-gap",
        Category::BrushTip,
        &tr!("adjust-brush-density-by-gap"),
        Some(&tr!("adjust-brush-density-by-gap-hint")),
        &mut by_gap,
    ) {
        edit_behaviours(sub, |b| b.gap_density = by_gap);
    }
}

/// The tip's pictures, each with a button that takes it out, and the
/// picker that adds one: a search over the brush tips' names and tags.
fn tip_list(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    sub: &mut SubTool,
    work: &mut BrushWork,
    tips: &[u32],
) {
    let added = rows
        .row(ui, "tip-list", Category::BrushTip, |ui, theme, enabled| {
            let mut out: Option<Vec<u32>> = None;
            for (at, id) in tips.iter().enumerate() {
                let name = crate::brush::shape(*id)
                    .map(|shape| shape.name.clone())
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    let remove = tr!("remove-brush-tip", name = name.as_str());
                    if sue_wm::ui::icon_button(("sue-paint-tip-remove", at), Icon::Delete, &remove)
                        .quiet()
                        .enabled(enabled)
                        .show(ui, theme)
                        .clicked()
                    {
                        let mut left = tips.to_vec();
                        left.remove(at);
                        out = Some(left);
                    }
                    ui.label(
                        RichText::new(&name)
                            .size(theme.text_size(TextRole::Caption))
                            .color(theme.text),
                    );
                });
            }
            let add = tr!("add-brush-tip");
            if sue_wm::ui::button("sue-paint-tip-add", &add)
                .enabled(enabled)
                .show(ui, theme)
                .on_hover_text(&add)
                .clicked()
            {
                work.tip_search = match work.tip_search {
                    Some(_) => None,
                    None => Some(String::new()),
                };
            }
            if let Some(query) = work.tip_search.as_mut() {
                ui.add_space(theme.space(Space::S1));
                let label = tr!("search-brush-tips");
                let _ = sue_wm::ui::input::text_field("sue-paint-tip-search", query)
                    .label(&label)
                    .show(ui, theme)
                    .on_hover_text(tr!("search-brush-tips-hint"));
                let found: Vec<&crate::brush_rows::TipChoice> = work
                    .tips
                    .iter()
                    .filter(|tip| tip.matches(query))
                    .take(12)
                    .collect();
                if found.is_empty() {
                    ui.label(
                        RichText::new(tr!("no-brush-tip-found"))
                            .size(theme.text_size(TextRole::Caption))
                            .color(theme.text_muted()),
                    );
                }
                for tip in found {
                    let shown = if tip.tags.is_empty() {
                        tip.name.clone()
                    } else {
                        format!("{} ({})", tip.name, tip.tags.join(", "))
                    };
                    let row = sue_wm::ui::list_row(("sue-paint-tip-pick", tip.id), &shown)
                        .show(ui, theme);
                    if row.response.on_hover_text(&shown).clicked() && enabled {
                        let mut more = tips.to_vec();
                        more.push(tip.id);
                        out = Some(more);
                    }
                }
            }
            out
        })
        .flatten();
    if let Some(list) = added {
        if list.len() > tips.len() {
            // A tip was added: the picker has done its work.
            work.tip_search = None;
        }
        set_tips(sub, list);
    }
}

/// Makes `list` the tip's pictures: the first is the tip's own shape, and
/// a list of more than one is kept in the preset rows.
fn set_tips(sub: &mut SubTool, list: Vec<u32>) {
    match list.first() {
        None => {
            sub.tip.shape = 0;
            sub.tip.shape_count = 1;
        }
        Some(first) => {
            sub.tip.shape = *first;
            sub.tip.shape_count = 1;
        }
    }
    edit_rows(sub, |r| {
        r.tips = if list.len() > 1 { list } else { Vec::new() };
        true
    });
}

/// What people read for a particle direction.
fn direction_name(direction: ParticleDirection) -> String {
    match direction {
        ParticleDirection::Brush => tr!("particle-direction-brush"),
        ParticleDirection::Line => tr!("particle-direction-line"),
        ParticleDirection::Random => tr!("particle-direction-random"),
    }
}

/// Spraying effect (00:30:58, FGV-25): on or off, particle size (linked
/// to the brush's size or in pixels), particle density, spray deviation
/// and the particles' direction.
pub(super) fn spray_rows(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    let b = crate::canvas::engine::behaviours_of(sub);
    let mut on = b.spray.is_some();
    if rows.check(
        ui,
        "spray",
        Category::SprayingEffect,
        &tr!("spraying-effect"),
        Some(&tr!("spraying-effect-hint")),
        &mut on,
    ) {
        edit_behaviours(sub, |b| b.spray = on.then(Spray::default));
    }
    let Some(mut spray) = crate::canvas::engine::behaviours_of(sub).spray else {
        return;
    };
    let before = spray;
    if spray.linked {
        rows.percent(
            ui,
            "particle-size",
            Category::SprayingEffect,
            &tr!("particle-size"),
            &mut spray.size,
            1.0,
            100.0,
        );
    } else {
        rows.slider(
            ui,
            "particle-size",
            Category::SprayingEffect,
            &tr!("particle-size"),
            &mut spray.size,
            0.5..=200.0,
            &tr!("px"),
            true,
        );
    }
    if rows.check(
        ui,
        "particle-linked",
        Category::SprayingEffect,
        &tr!("particle-size-linked"),
        Some(&tr!("particle-size-linked-hint")),
        &mut spray.linked,
    ) {
        // The same particles in the other measure, at 100 pixels.
        spray.size = if spray.linked {
            (spray.size / 100.0).clamp(0.01, 1.0)
        } else {
            (spray.size * 100.0).clamp(0.5, 200.0)
        };
    }
    rows.slider(
        ui,
        "particle-density",
        Category::SprayingEffect,
        &tr!("particle-density"),
        &mut spray.density,
        1.0..=256.0,
        "",
        true,
    );
    rows.percent(
        ui,
        "spray-deviation",
        Category::SprayingEffect,
        &tr!("spray-deviation"),
        &mut spray.deviation,
        0.0,
        100.0,
    );
    let directions: Vec<String> = ParticleDirection::ALL
        .iter()
        .map(|d| direction_name(*d))
        .collect();
    let mut at = ParticleDirection::ALL
        .iter()
        .position(|d| *d == spray.direction)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "particle-direction",
        Category::SprayingEffect,
        &tr!("particle-direction"),
        &mut at,
        &directions,
    ) {
        spray.direction = ParticleDirection::ALL[at];
    }
    if spray != before {
        spray.density = spray.density.round().max(1.0);
        edit_behaviours(sub, |b| b.spray = Some(spray));
    }
}

/// What people read for a repeat method.
fn order_name(order: TipOrder) -> String {
    match order {
        TipOrder::Repeat => tr!("repeat-method-repeat"),
        TipOrder::BackAndForth => tr!("repeat-method-back-and-forth"),
        TipOrder::Clamp => tr!("repeat-method-clamp"),
        TipOrder::Random => tr!("repeat-method-random"),
    }
}

/// What people read for a gap preset.
fn gap_name(gap: Gap) -> String {
    match gap {
        Gap::Fixed => tr!("gap-fixed"),
        Gap::Narrow => tr!("gap-narrow"),
        Gap::Normal => tr!("gap-normal"),
        Gap::Wide => tr!("gap-wide"),
    }
}

/// Stroke (00:31:12, FGV-26): the gap presets (and a fixed gap's
/// pixels), continuous spraying and how fast, the repeat method of a tip
/// of several pictures, and blending a color tip's dabs with Darken.
pub(super) fn stroke_rows(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    let mut r = preset_rows_of(sub);
    let names: Vec<String> = Gap::ALL.iter().map(|gap| gap_name(*gap)).collect();
    let mut at = Gap::ALL.iter().position(|gap| *gap == r.gap).unwrap_or(2);
    let mut changed = false;
    if rows.choice(
        ui,
        "gap-preset",
        Category::Stroke,
        &tr!("gap"),
        &mut at,
        &names,
    ) {
        r.gap = Gap::ALL[at];
        changed = true;
    }
    if r.gap == Gap::Fixed {
        changed |= rows.slider(
            ui,
            "gap-fixed",
            Category::Stroke,
            &tr!("fixed-gap"),
            &mut r.fixed_gap,
            1.0..=500.0,
            &tr!("px"),
            true,
        );
    }
    if changed {
        sub.brush_rows.preset = Some(r);
    }
    let b = crate::canvas::engine::behaviours_of(sub);
    let mut continuous = b.continuous.is_some();
    if rows.check(
        ui,
        "continuous-spraying",
        Category::Stroke,
        &tr!("continuous-spraying"),
        Some(&tr!("continuous-spraying-hint")),
        &mut continuous,
    ) {
        edit_behaviours(sub, |b| b.continuous = continuous.then_some(60.0));
    }
    if let Some(mut rate) = crate::canvas::engine::behaviours_of(sub).continuous
        && rows.slider(
            ui,
            "continuous-rate",
            Category::Stroke,
            &tr!("continuous-spraying-rate"),
            &mut rate,
            1.0..=240.0,
            &tr!("per-second"),
            false,
        )
    {
        edit_behaviours(sub, |b| b.continuous = Some(rate.round().max(1.0)));
    }
    if tips_of(sub).len() > 1 {
        let orders: Vec<String> = TipOrder::ALL.iter().map(|o| order_name(*o)).collect();
        let mut at = TipOrder::ALL
            .iter()
            .position(|o| *o == b.tip_order)
            .unwrap_or(0);
        if rows.choice(
            ui,
            "repeat-method",
            Category::Stroke,
            &tr!("repeat-method"),
            &mut at,
            &orders,
        ) {
            edit_behaviours(sub, |b| b.tip_order = TipOrder::ALL[at]);
        }
    }
    if sub.tip.keep_colors {
        let mut darken = b.tip_blend == TipBlend::Darken;
        if rows.check(
            ui,
            "tip-blend-darken",
            Category::Stroke,
            &tr!("blend-brush-tips-with-darken"),
            None,
            &mut darken,
        ) {
            edit_behaviours(sub, |b| {
                b.tip_blend = if darken {
                    TipBlend::Darken
                } else {
                    TipBlend::Over
                };
            });
        }
    }
}

/// Erase (00:32:46): an eraser erases every layer it may, not only the
/// one being edited (Erase all layers).
pub(super) fn erase_rows(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    let mut all = crate::canvas::engine::behaviours_of(sub).all_layers;
    if rows.check(
        ui,
        "erase-all-layers",
        Category::Erase,
        &tr!("erase-all-layers"),
        Some(&tr!("erase-all-layers-hint")),
        &mut all,
    ) {
        edit_behaviours(sub, |b| b.all_layers = all);
    }
}

/// Correction beside stabilization and the level: Sharp angles
/// (00:32:46), post correction by speed, by scale and as Bezier curves
/// (00:33:03, FGV-29), and Enable snapping (00:33:16).
pub(super) fn correction_rows(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    if sub.correction > 0 {
        let r = &mut sub.brush_rows;
        rows.check(
            ui,
            "correction-by-speed",
            Category::Correction,
            &tr!("post-correction-by-speed"),
            Some(&tr!("post-correction-by-speed-hint")),
            &mut r.correction_by_speed,
        );
        rows.check(
            ui,
            "correction-by-scale",
            Category::Correction,
            &tr!("post-correction-by-scale"),
            Some(&tr!("post-correction-by-scale-hint")),
            &mut r.correction_by_scale,
        );
        rows.check(
            ui,
            "correction-bezier",
            Category::Correction,
            &tr!("post-correction-bezier"),
            Some(&tr!("post-correction-bezier-hint")),
            &mut r.bezier,
        );
    }
    let mut sharp = crate::canvas::engine::behaviours_of(sub).sharp_angles;
    if rows.check(
        ui,
        "sharp-angles",
        Category::Correction,
        &tr!("sharp-angles"),
        Some(&tr!("sharp-angles-hint")),
        &mut sharp,
    ) {
        edit_behaviours(sub, |b| b.sharp_angles = sharp);
    }
    rows.check(
        ui,
        "snapping",
        Category::Correction,
        &tr!("enable-snapping"),
        Some(&tr!("enable-snapping-hint")),
        &mut sub.brush_rows.snapping,
    );
}

/// Anti-overflow (00:33:22, FGV-31): the paint stays inside the lines of
/// the reference layer around where the stroke starts, with how opaque a
/// line must be and how far past it the area reaches.
pub(super) fn overflow_rows(ui: &mut Ui, rows: &mut Rows<'_>, tool: Tool, sub: &mut SubTool) {
    if tool == Tool::Eraser {
        return;
    }
    let mut on = sub.brush_rows.overflow.is_some();
    if rows.check(
        ui,
        "anti-overflow",
        Category::AntiOverflow,
        &tr!("do-not-cross-reference-lines"),
        Some(&tr!("do-not-cross-reference-lines-hint")),
        &mut on,
    ) {
        sub.brush_rows.overflow = on.then(AntiOverflow::default);
    }
    if let Some(o) = &mut sub.brush_rows.overflow {
        rows.percent(
            ui,
            "anti-overflow-tolerance",
            Category::AntiOverflow,
            &tr!("tolerance"),
            &mut o.tolerance,
            1.0,
            100.0,
        );
        let mut scaling = o.scaling as f32;
        if rows.slider(
            ui,
            "anti-overflow-scaling",
            Category::AntiOverflow,
            &tr!("area-scaling"),
            &mut scaling,
            -20.0..=20.0,
            &tr!("px"),
            false,
        ) {
            o.scaling = scaling.round() as i32;
        }
    }
}
