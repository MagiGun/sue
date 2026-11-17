//! Tool Property for the tools that paint: the pens, pencils and
//! brushes, the airbrush, decoration, the eraser and the figure tools,
//! and the Blend tool beside them.

use egui::Ui;

use super::brush::{color_jitter, dynamic_row, starting_and_ending};
use super::shape;
use super::{Category, Rows};
use crate::brush::{AntiAlias, Tip};
use crate::brush_rows::BrushWork;
use crate::tools::{SubTool, Tool};

/// A painting tool's settings: its size, opacity, hardness and
/// anti-aliasing, its smoothing, then its tip.
/// Returns a command a row asks for (Register of the brush shapes).
pub(super) fn paint_properties(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    tool: Tool,
    sub: &mut SubTool,
    work: &mut BrushWork,
) -> Option<&'static str> {
    if tool == Tool::Eraser {
        // How it erases lines on vector layers.
        let names = super::names(&crate::vector::Erase::NAMES);
        rows.choice_u8(
            ui,
            "vector-erase",
            Category::Erase,
            &tr!("vector"),
            &mut sub.kind,
            &names,
        );
        shape::erase_rows(ui, rows, sub);
    }
    let tip = &mut sub.tip;
    let (mut screen_size, mut at_least_one) = (tip.screen_size, tip.at_least_one);
    dynamic_row(
        ui,
        rows,
        "brush-size",
        Category::BrushSize,
        &tr!("brush-size"),
        &mut tip.size,
        0.5..=1000.0,
        &tr!("px"),
        true,
        &mut tip.size_by,
        |ui, theme| {
            ui.separator();
            for (id, label, hint, value) in [
                (
                    "screen-size",
                    tr!("specify-by-size-on-screen"),
                    tr!("brush-looks-same-size-any-zoom"),
                    &mut screen_size,
                ),
                (
                    "at-least-one",
                    tr!("at-least-1-pixel"),
                    tr!("light-pressure-never-thins-line-below"),
                    &mut at_least_one,
                ),
            ] {
                if sue_wm::ui::checkbox(("sue-paint-size-option", id), &label, *value)
                    .show(ui, theme)
                    .on_hover_text(hint)
                    .clicked()
                {
                    *value = !*value;
                }
            }
        },
    );
    tip.screen_size = screen_size;
    tip.at_least_one = at_least_one;
    let mut opacity = tip.opacity * 100.0;
    if dynamic_row(
        ui,
        rows,
        "opacity",
        Category::Ink,
        &tr!("opacity"),
        &mut opacity,
        0.0..=100.0,
        " %",
        false,
        &mut tip.opacity_by,
        |_, _| {},
    ) {
        tip.opacity = opacity / 100.0;
    }
    rows.percent(
        ui,
        "hardness",
        Category::Ink,
        &tr!("hardness"),
        &mut tip.hardness,
        0.0,
        100.0,
    );

    let tip = &mut sub.tip;
    let levels: Vec<String> = AntiAlias::ALL.iter().map(|level| level.label()).collect();
    let mut level = AntiAlias::ALL
        .iter()
        .position(|found| *found == tip.aa)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "anti-aliasing",
        Category::AntiAliasing,
        &tr!("anti-aliasing"),
        &mut level,
        &levels,
    ) {
        tip.aa = AntiAlias::ALL[level];
    }
    rows.slider_u32(
        ui,
        "stabilization",
        Category::Correction,
        &tr!("stabilization"),
        &mut sub.stabilization,
        0..=100,
    );
    rows.check(
        ui,
        "adjust-by-speed",
        Category::Correction,
        &tr!("adjust-by-speed"),
        Some(&tr!("less-smoothing-as-pen-moves-faster")),
        &mut sub.adjust_by_speed,
    );
    rows.slider_u32(
        ui,
        "post-correction",
        Category::Correction,
        &tr!("correction"),
        &mut sub.correction,
        0..=10,
    );
    rows.check(
        ui,
        "flick",
        Category::Correction,
        &tr!("flick-into-a-point"),
        Some(&tr!("quick-stroke-carries-along-pens-momentum")),
        &mut sub.flick,
    );
    // Clip Studio Paint 5.0's Smart Shape.
    rows.check(
        ui,
        "smart-shape",
        Category::Correction,
        &tr!("smart-shape"),
        Some(&tr!("smart-shape-hint")),
        &mut sub.smart_shape,
    );
    shape::correction_rows(ui, rows, sub);
    let mut command = None;
    if tool != Tool::Figure {
        command = shape::shape_rows(ui, rows, sub, work);
        shape::tip_rows(ui, rows, sub, work);
    }
    tip_properties(ui, rows, &mut sub.tip);
    // After Color mixing, which opens the mixing rows.
    shape::ink_rows(ui, rows, sub);
    if tool != Tool::Figure {
        shape::spray_rows(ui, rows, sub);
        shape::stroke_rows(ui, rows, sub);
        behaviour_properties(ui, rows, sub);
        shape::overflow_rows(ui, rows, tool, sub);
    }
    if tool == Tool::Figure {
        rows.hint(ui, &tr!("drag-canvas-draw-shape"));
    }
    command
}

/// The brush tip: its shape and how it turns and scatters, spacing, build
/// up, color mixing, paper grain, starting and ending, and color jitter.
fn tip_properties(ui: &mut Ui, rows: &mut Rows<'_>, tip: &mut Tip) {
    if tip.shape != 0 {
        // Clip Studio Paint's Direction and Spray: how each shape turns
        // and how far it falls from the line.
        rows.check(
            ui,
            "ribbon",
            Category::BrushTip,
            &tr!("lay-as-a-ribbon"),
            Some(&tr!("the-shape-bends-along-the-whole-stroke")),
            &mut tip.ribbon,
        );
        rows.check(
            ui,
            "follow-line",
            Category::BrushTip,
            &tr!("turn-along-the-line"),
            None,
            &mut tip.follow,
        );
        rows.check(
            ui,
            "follow-tilt",
            Category::BrushTip,
            &tr!("turn-with-pen-tilt"),
            Some(&tr!("a-flat-tip-turns-the-way-the-pen-leans")),
            &mut tip.follow_tilt,
        );
        rows.slider(
            ui,
            "tip-angle",
            Category::BrushTip,
            &tr!("angle-2"),
            &mut tip.angle,
            -180.0..=180.0,
            "\u{b0}",
            false,
        );
        rows.percent(
            ui,
            "scatter",
            Category::BrushTip,
            &tr!("scatter"),
            &mut tip.scatter,
            0.0,
            300.0,
        );
        rows.percent(
            ui,
            "size-jitter",
            Category::BrushTip,
            &tr!("size-jitter"),
            &mut tip.size_jitter,
            0.0,
            100.0,
        );
        rows.percent(
            ui,
            "turn-jitter",
            Category::BrushTip,
            &tr!("turn-jitter"),
            &mut tip.turn_jitter,
            0.0,
            100.0,
        );
    }
    if tip.dabbed() {
        rows.percent(
            ui,
            "spacing",
            Category::Stroke,
            &tr!("spacing"),
            &mut tip.spacing,
            1.0,
            200.0,
        );
    }
    rows.check(
        ui,
        "build-up",
        Category::Stroke,
        &tr!("build-up-airbrush"),
        None,
        &mut tip.build_up,
    );
    if tip.build_up {
        rows.percent(
            ui,
            "flow",
            Category::Stroke,
            &tr!("flow"),
            &mut tip.flow,
            1.0,
            100.0,
        );
    }
    if !tip.eraser {
        rows.percent(
            ui,
            "color-mixing",
            Category::Ink,
            &tr!("color-mixing"),
            &mut tip.mix,
            0.0,
            100.0,
        );
    }
    rows.percent(
        ui,
        "paper-grain",
        Category::Texture,
        &tr!("paper-grain"),
        &mut tip.grain,
        0.0,
        100.0,
    );
    starting_and_ending(ui, rows, tip);
    let stamps = tip.dabbed();
    color_jitter(ui, rows, &mut tip.jitter, stamps);
}

/// The paint behaviours a brush's text has no word for, set per sub tool
/// (`SubTool::behaviours`): the paper texture's choice and how it is
/// laid (full guide 00:31:45, FGV-27), the watercolor edge (00:12:49,
/// 00:32:23) and the dual brush (00:32:06, FGV-28).
fn behaviour_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    use sue_brush::paint::{DualBrush, DualMode, PaperChoice, TextureChoice, WatercolorEdge};
    use sue_brush::texture::{TextureMode, TextureSettings};
    let before = crate::canvas::engine::behaviours_of(sub);
    let mut b = before.clone();
    // The paper: none, one of Sue's own, a material registered for it
    // (full guide FGV-48), or a person's picture kept as it is.
    let papers = sue_brush::texture::BUILTIN;
    let own = crate::material_props::papers();
    let mut names = vec![tr!("texture-none")];
    names.extend(papers.iter().map(|name| paper_label(name)));
    names.extend(own.iter().map(|(name, _)| name.clone()));
    let picture = match b.texture.as_ref().map(|t| &t.paper) {
        Some(PaperChoice::Picture(hash)) if !own.iter().any(|(_, h)| h == hash) => Some(*hash),
        _ => None,
    };
    if picture.is_some() {
        names.push(tr!("texture-own-picture"));
    }
    let mut chosen = match b.texture.as_ref().map(|t| &t.paper) {
        None => 0,
        Some(PaperChoice::Builtin(name)) => papers
            .iter()
            .position(|found| found == name)
            .map_or(0, |at| at + 1),
        Some(PaperChoice::Picture(hash)) => own
            .iter()
            .position(|(_, h)| h == hash)
            .map_or(papers.len() + own.len() + 1, |at| papers.len() + 1 + at),
    };
    if rows.choice(
        ui,
        "texture-paper",
        Category::Texture,
        &tr!("texture"),
        &mut chosen,
        &names,
    ) {
        let settings = b
            .texture
            .as_ref()
            .map_or_else(TextureSettings::default, |t| t.settings);
        b.texture = match chosen {
            0 => None,
            at if at <= papers.len() => Some(TextureChoice {
                paper: PaperChoice::Builtin(papers[at - 1].to_owned()),
                settings,
            }),
            at if at <= papers.len() + own.len() => Some(TextureChoice {
                paper: PaperChoice::Picture(own[at - papers.len() - 1].1),
                settings,
            }),
            _ => b.texture.clone(),
        };
    }
    if let Some(texture) = &mut b.texture {
        let s = &mut texture.settings;
        let mut scale = s.scale * 100.0;
        if rows.slider(
            ui,
            "texture-scale",
            Category::Texture,
            &tr!("texture-scale-ratio"),
            &mut scale,
            10.0..=400.0,
            " %",
            false,
        ) {
            s.scale = scale / 100.0;
        }
        rows.slider(
            ui,
            "texture-rotation",
            Category::Texture,
            &tr!("texture-rotation-angle"),
            &mut s.rotation,
            -180.0..=180.0,
            "\u{b0}",
            false,
        );
        rows.percent(
            ui,
            "texture-brightness",
            Category::Texture,
            &tr!("texture-brightness"),
            &mut s.brightness,
            -100.0,
            100.0,
        );
        rows.percent(
            ui,
            "texture-contrast",
            Category::Texture,
            &tr!("texture-contrast"),
            &mut s.contrast,
            -100.0,
            100.0,
        );
        rows.check(
            ui,
            "texture-invert",
            Category::Texture,
            &tr!("texture-invert"),
            None,
            &mut s.invert,
        );
        rows.check(
            ui,
            "texture-emphasize",
            Category::Texture,
            &tr!("texture-emphasize-density"),
            Some(&tr!("texture-emphasize-density-hint")),
            &mut s.emphasize,
        );
        let modes = TextureMode::ALL;
        let labels: Vec<String> = modes.iter().map(|mode| texture_mode_label(*mode)).collect();
        let mut at = modes.iter().position(|mode| *mode == s.mode).unwrap_or(0);
        if rows.choice(
            ui,
            "texture-mode",
            Category::Texture,
            &tr!("texture-mode"),
            &mut at,
            &labels,
        ) {
            s.mode = modes[at];
        }
        rows.check(
            ui,
            "texture-per-dab",
            Category::Texture,
            &tr!("texture-apply-by-each-plot"),
            Some(&tr!("texture-apply-by-each-plot-hint")),
            &mut s.per_dab,
        );
    }
    // The watercolor edge.
    let mut edge_on = b.watercolor.is_some();
    if rows.check(
        ui,
        "watercolor-edge",
        Category::WatercolorEdge,
        &tr!("watercolor-edge"),
        Some(&tr!("watercolor-edge-hint")),
        &mut edge_on,
    ) {
        b.watercolor = edge_on.then(WatercolorEdge::default);
    }
    if let Some(edge) = &mut b.watercolor {
        rows.slider(
            ui,
            "watercolor-range",
            Category::WatercolorEdge,
            &tr!("watercolor-range"),
            &mut edge.range,
            0.0..=100.0,
            &tr!("px"),
            false,
        );
        rows.percent(
            ui,
            "watercolor-opacity",
            Category::WatercolorEdge,
            &tr!("watercolor-opacity"),
            &mut edge.opacity,
            0.0,
            100.0,
        );
        rows.percent(
            ui,
            "watercolor-darkness",
            Category::WatercolorEdge,
            &tr!("watercolor-darkness"),
            &mut edge.darkness,
            0.0,
            100.0,
        );
        rows.slider(
            ui,
            "watercolor-blur",
            Category::WatercolorEdge,
            &tr!("watercolor-blurring-width"),
            &mut edge.blur,
            0.0..=20.0,
            &tr!("px"),
            false,
        );
        rows.check(
            ui,
            "watercolor-after",
            Category::WatercolorEdge,
            &tr!("watercolor-process-after-stroke"),
            Some(&tr!("watercolor-process-after-stroke-hint")),
            &mut edge.after_stroke,
        );
    }
    // The dual brush: a second round tip the stroke is laid through.
    let mut dual_on = b.dual.is_some();
    if rows.check(
        ui,
        "dual-brush",
        Category::DualBrush,
        &tr!("dual-brush"),
        Some(&tr!("dual-brush-hint")),
        &mut dual_on,
    ) {
        b.dual = dual_on.then(DualBrush::default);
    }
    if let Some(dual) = &mut b.dual {
        let modes = DualMode::ALL;
        let labels: Vec<String> = modes.iter().map(|mode| dual_mode_label(*mode)).collect();
        let mut at = modes
            .iter()
            .position(|mode| *mode == dual.mode)
            .unwrap_or(0);
        if rows.choice(
            ui,
            "dual-mode",
            Category::DualBrush,
            &tr!("dual-brush-mode"),
            &mut at,
            &labels,
        ) {
            dual.mode = modes[at];
        }
        rows.check(
            ui,
            "dual-linked",
            Category::DualBrush,
            &tr!("dual-brush-link-to-main-size"),
            None,
            &mut dual.linked,
        );
        if dual.linked {
            rows.percent(
                ui,
                "dual-size",
                Category::DualBrush,
                &tr!("dual-brush-size"),
                &mut dual.size,
                1.0,
                200.0,
            );
        } else {
            rows.slider(
                ui,
                "dual-size",
                Category::DualBrush,
                &tr!("dual-brush-size"),
                &mut dual.size,
                0.5..=1000.0,
                &tr!("px"),
                true,
            );
        }
        rows.percent(
            ui,
            "dual-spacing",
            Category::DualBrush,
            &tr!("dual-brush-spacing"),
            &mut dual.spacing,
            1.0,
            200.0,
        );
        rows.percent(
            ui,
            "dual-scatter",
            Category::DualBrush,
            &tr!("dual-brush-scatter"),
            &mut dual.scatter,
            0.0,
            300.0,
        );
        rows.percent(
            ui,
            "dual-hardness",
            Category::DualBrush,
            &tr!("dual-brush-hardness"),
            &mut dual.hardness,
            0.0,
            100.0,
        );
    }
    if b != before {
        sub.behaviours = Some(b);
    }
}

/// What people read for one of Sue's papers.
fn paper_label(name: &str) -> String {
    match name {
        "grain" => tr!("paper-grain-name"),
        "canvas" => tr!("paper-canvas"),
        "cold-press" => tr!("paper-cold-press"),
        "rough" => tr!("paper-rough"),
        other => other.to_owned(),
    }
}

/// What people read for a texture mode.
fn texture_mode_label(mode: sue_brush::texture::TextureMode) -> String {
    use sue_brush::texture::TextureMode;
    match mode {
        TextureMode::Multiply => tr!("texture-mode-multiply"),
        TextureMode::Subtract => tr!("texture-mode-subtract"),
        TextureMode::Height => tr!("texture-mode-height"),
        TextureMode::Darken => tr!("texture-mode-darken"),
        TextureMode::Overlay => tr!("texture-mode-overlay"),
    }
}

/// What people read for a dual brush mode.
fn dual_mode_label(mode: sue_brush::paint::DualMode) -> String {
    use sue_brush::paint::DualMode;
    match mode {
        DualMode::Multiply => tr!("texture-mode-multiply"),
        DualMode::Darken => tr!("texture-mode-darken"),
        DualMode::Subtract => tr!("texture-mode-subtract"),
        DualMode::Overlay => tr!("texture-mode-overlay"),
        DualMode::Height => tr!("texture-mode-height"),
    }
}

/// The Blend tool: its size and strength, and what the sub tool does.
pub(super) fn blend_properties(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    rows.slider(
        ui,
        "blend-size",
        Category::BrushSize,
        &tr!("brush-size"),
        &mut sub.tip.size,
        2.0..=600.0,
        &tr!("px"),
        true,
    );
    rows.percent(
        ui,
        "strength",
        Category::Ink,
        &tr!("strength"),
        &mut sub.tip.opacity,
        1.0,
        100.0,
    );
    // How far out from the middle Liquify and the blenders work in full
    // before they fall off (crash course CC-4 and CC-5).
    let blender = crate::canvas::engine::is_blender(sub);
    if blender || crate::blend::is_liquify(sub.mode) {
        rows.percent(
            ui,
            "blend-hardness",
            Category::Ink,
            &tr!("hardness"),
            &mut sub.tip.hardness,
            0.0,
            100.0,
        );
    }
    // Liquify's mode, one of its seven (`blend::LiquifyMode`).
    if let Some(mode) = crate::blend::LiquifyMode::of_mode(sub.mode) {
        let modes = crate::blend::LiquifyMode::ALL;
        let names: Vec<String> = modes.iter().map(|found| found.label()).collect();
        let mut chosen = modes.iter().position(|found| *found == mode).unwrap_or(0);
        if rows.choice(
            ui,
            "liquify-mode",
            Category::Operation,
            &tr!("mode"),
            &mut chosen,
            &names,
        ) {
            sub.mode = modes[chosen].mode();
        }
    }
    if blender {
        rows.hint(
            ui,
            &match sub.name.as_str() {
                "Painterly blender" => tr!("blender-painterly-hint"),
                "Wet bleed blender" => tr!("blender-wet-bleed-hint"),
                "Textured blender" => tr!("blender-textured-hint"),
                "Soothing watercolor" => tr!("blender-soothing-watercolor-hint"),
                _ => tr!("blender-blend-hint"),
            },
        );
        return;
    }
    rows.hint(
        ui,
        &match sub.mode {
            0 => tr!("softens-what-under-brush"),
            1 => tr!("drags-color-along-the-stroke"),
            2 => tr!("moves-picture-along-stroke"),
            3 => tr!("swells-picture-brushs-middle"),
            4 => tr!("draws-picture-into-brushs-middle"),
            8 | 9 => tr!("moves-picture-across-stroke"),
            crate::blend::COPY_STAMP => tr!("copies-picture-from-source"),
            _ => tr!("turns-picture-around-brushs-middle"),
        },
    );
}
