//! The Edit menu's commands: undo, the clipboard, fills, transforms,
//! alignment, the canvas's size and materials.
//!
//! Each command is one entry of [`COMMANDS`] with its own `run` and its
//! own `state`; commands that do the same with one difference (the six
//! alignments, the two kinds of transform) share a function and tell
//! themselves apart by their name.

use sue_core::Value;
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::{Basis, when};
use crate::color;
use crate::dialogs::Dialog;
use crate::doc;
use crate::filters::Filter;
use crate::select::Selection;
use crate::studio::{SuePaint, mirror};

// How an item looks right now.

/// Works while a canvas is open.
fn has_doc(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    let Basis { has_doc, .. } = paint.basis(facts.arena);
    when(has_doc)
}

/// Works on a raster layer.
fn on_raster(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    let Basis { raster, .. } = paint.basis(facts.arena);
    when(raster)
}

/// Works while the canvas has a selection.
fn with_selection(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    let Basis { selection, .. } = paint.basis(facts.arena);
    when(selection)
}

/// Works while something was copied and a canvas is open.
fn with_clipboard(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    let Basis { has_doc, .. } = paint.basis(facts.arena);
    when(has_doc && paint.clipboard.is_some())
}

/// Works while there is a step to undo.
fn can_undo(_: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(facts.history.can_undo())
}

/// Works while there is a step to redo.
fn can_redo(_: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(facts.history.can_redo())
}

// What each command does.

/// Edit, Undo and Edit, Redo: the words being typed are kept first.
fn step_history(cx: &mut AppletContext<'_>, redo: bool) {
    cx.typing.flush(cx.history);
    let result = if redo {
        cx.history.redo(cx.world)
    } else {
        cx.history.undo(cx.world)
    };
    if let Err(error) = result {
        cx.fail(tr!("undo-failed", error = error));
    }
}

fn undo(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    step_history(cx, false);
}

fn redo(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    step_history(cx, true);
}

fn cut(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.copy(cx) {
        paint.run_clear(cx, false, &tr!("cut"));
    }
}

fn copy(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.copy(cx) {
        cx.notify(tr!("copied"));
    }
}

fn paste(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.paste(cx, &tr!("pasted"));
}

/// Paste to Shown Position: the copy lands in the middle of the view.
fn paste_here(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let (Some(doc), Some(clip)) = (paint.doc(), paint.clipboard.as_ref()) else {
        return;
    };
    let size = cx
        .world
        .arena
        .get_by_id(doc.picture)
        .ok()
        .and_then(picture::size)
        .unwrap_or([1, 1]);
    let center = doc
        .view
        .to_picture(paint.viewport, size, paint.viewport.center());
    let origin = [
        (center.x - clip.raster.width() as f32 / 2.0)
            .round()
            .max(0.0) as u32,
        (center.y - clip.raster.height() as f32 / 2.0)
            .round()
            .max(0.0) as u32,
    ];
    let saved = clip.origin;
    if let Some(clip) = paint.clipboard.as_mut() {
        clip.origin = origin;
    }
    paint.paste(cx, &tr!("pasted"));
    if let Some(clip) = paint.clipboard.as_mut() {
        clip.origin = saved;
    }
}

/// Delete: the lines the Operation tool picked, else what is selected.
fn clear(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.toolbox.tool() == crate::tools::Tool::Operation
        && paint
            .picked_lines
            .as_ref()
            .is_some_and(|(_, picked)| !picked.is_empty())
    {
        paint.delete_picked_lines(cx);
    } else {
        paint.run_clear(cx, false, &tr!("clear"));
    }
}

fn clear_outside(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.run_clear(cx, true, &tr!("clear-outside-selection-2"));
}

fn smart_smoothing(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.filter_dialog(cx, Filter::SmartSmooth { strength: 0.5 });
}

/// Fill: the selection (or the whole layer) in the drawing color.
fn fill(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let rgb = color::unit(paint.drawing_color());
    let Some(selection) = paint.selection_or_all(&cx.world.arena) else {
        return;
    };
    let size = selection.size();
    let alpha_lock = paint
        .layer_info(&cx.world.arena)
        .is_some_and(|info| info.alpha_locked);
    let offset = paint
        .layer_info(&cx.world.arena)
        .map_or([0, 0], |info| info.style.offset);
    let mask: Vec<u8> = (0..size[1])
        .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
        .map(|(x, y)| (selection.at(x, y) * 255.0).round() as u8)
        .collect();
    paint.rewrite(cx, &tr!("fill"), |pixels, width, height| {
        crate::fill::paint_mask(
            pixels,
            [width, height],
            offset,
            &mask,
            size,
            rgb,
            1.0,
            alpha_lock,
        );
    });
}

fn advanced_fill(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Err(error) = paint.paintable(&cx.world.arena) {
        cx.fail(error);
        return;
    }
    paint.dialog = Some(Dialog::AdvancedFill);
}

fn recolor(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let rgb = paint.drawing_color();
    paint.filter_now(cx, Filter::Recolor { rgb });
}

fn outline_selection(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.doc().is_some_and(|doc| doc.selection.is_some()) {
        paint.dialog = Some(Dialog::SelectBy(3, 4.0));
    } else {
        cx.fail(tr!("select-area-keep-first"));
    }
}

fn luma_to_alpha(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let rgb = paint.drawing_color();
    paint.filter_now(cx, Filter::BrightnessToOpacity { rgb });
}

/// Scale/Rotate, Scale and Rotate: the handles that keep the box square.
fn transform(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.start_transform(cx, false);
}

/// Free Transform, Distort, Skew and Perspective: each corner on its own.
fn free_transform(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.start_transform(cx, true);
}

/// Puts the layer back as it was before the transform.
fn cancel_transform(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.transform.is_some() {
        paint.finish_transform(cx, false);
    }
}

fn flip_h(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.rewrite(cx, &tr!("flip-layer"), |p, w, h| {
        mirror(p, w, h, true, false)
    });
}

fn flip_v(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.rewrite(cx, &tr!("flip-layer"), |p, w, h| {
        mirror(p, w, h, false, true)
    });
}

fn rotate180(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.rewrite(cx, &tr!("rotate-layer"), |p, w, h| {
        mirror(p, w, h, true, true);
    });
}

/// Mesh Transformation: a 4 by 4 grid of points over the layer's
/// pixels, each dragged to bend what is near it.
fn mesh(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.start_warp(cx);
    if let Some(warp) = paint.warp.as_mut()
        && warp.pins.is_empty()
        && let Some([x0, y0, x1, y1]) = warp.bounds
    {
        for row in 0..4 {
            for column in 0..4 {
                let at = [
                    x0 as f32 + (x1 - x0) as f32 * column as f32 / 3.0,
                    y0 as f32 + (y1 - y0) as f32 * row as f32 / 3.0,
                ];
                warp.pins.push(crate::warp::Pin::at(at));
            }
        }
        cx.notify(tr!("mesh-transformation-drag-points-bend-layer"));
    }
}

fn puppet(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.start_warp(cx);
}

/// Edit, Align/Distribute: the six alignments, told apart by their
/// names' last part.
fn align(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    if let Some(edge) = call.name.strip_prefix("edit.align.") {
        paint.align_layer(edge, cx);
    }
}

/// The picture's size and resolution.
fn canvas_measures(paint: &SuePaint, cx: &AppletContext<'_>) -> Option<([u32; 2], f64)> {
    paint
        .picture()
        .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
        .and_then(|entity| {
            let dpi = entity
                .property(picture::DPI)
                .and_then(Value::as_f64)
                .unwrap_or(72.0);
            picture::size(entity).map(|size| (size, dpi))
        })
}

fn resolution(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some((size, dpi)) = canvas_measures(paint, cx) else {
        return;
    };
    paint.dialog = Some(Dialog::Resolution(crate::dialogs::ResolutionDialog {
        size,
        old: size,
        dpi,
        keep: true,
    }));
}

fn canvas_size(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some((size, _)) = canvas_measures(paint, cx) else {
        return;
    };
    paint.dialog = Some(Dialog::Canvas(crate::dialogs::CanvasDialog {
        size,
        old: size,
        anchor: 4,
    }));
}

/// Clip Studio Paint's Change canvas height: the canvas's size, for a
/// webtoon's length.
fn canvas_height(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.run("edit.canvas_size", cx);
}

fn crop(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let (Some(picture), Some(bounds)) = (
        paint.picture(),
        paint
            .doc()
            .and_then(|doc| doc.selection.as_ref())
            .and_then(Selection::bounds),
    ) else {
        cx.fail(tr!("select-area-keep-first"));
        return;
    };
    let [x0, y0, x1, y1] = bounds;
    match doc::resize_canvas(
        &cx.world.arena,
        picture,
        [x1 - x0, y1 - y0],
        [-i64::from(x0), -i64::from(y0)],
    ) {
        Ok(edit) => {
            let edit = sue_core::txn::Batch::new(tr!("crop")).then(edit);
            SuePaint::apply(cx, edit);
            if let Some(doc) = paint.doc_mut() {
                doc.set_selection(None);
            }
        }
        Err(error) => cx.fail(error),
    }
}

fn canvas_properties(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let arena = &cx.world.arena;
    let entity = arena.get_by_id(picture).ok();
    let paper = doc::layers(arena, picture)
        .into_iter()
        .find(|info| info.kind == LayerKind::Paper);
    let paper_color = paper
        .as_ref()
        .and_then(|info| arena.get_by_id(info.id).ok())
        .map(|entity| {
            let rgb = picture::paper_color(entity)
                .to_space(sue_core::ColorSpace::Srgb)
                .rgb;
            rgb.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
        });
    paint.dialog = Some(Dialog::CanvasProperties(crate::dialogs::CanvasProperties {
        picture,
        name: entity
            .map(|entity| entity.name().to_owned())
            .unwrap_or_default(),
        dpi: entity
            .and_then(|entity| entity.property(picture::DPI))
            .and_then(Value::as_f64)
            .unwrap_or(72.0),
        paper: paper.as_ref().map(|info| info.id),
        paper_color: paper_color.unwrap_or([255, 255, 255]),
        paper_shown: paper.as_ref().is_some_and(|info| info.style.visible),
    }));
}

/// Edit, Clear work history: this canvas's steps only, as each canvas
/// keeps its own.
fn clear_history(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    cx.typing.flush(cx.history);
    *cx.history = sue_core::UndoStack::new();
    cx.notify(tr!("work-history-cleared"));
}

/// Create Brush Tip from Image and Register Material, Brush Tip.
fn brush_tip(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.make_brush_tip(cx);
}

fn material_image(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let name = paint
        .layer_info(&cx.world.arena)
        .map_or_else(|| tr!("material"), |info| info.name);
    if paint.paintable(&cx.world.arena).is_ok() {
        paint.dialog = Some(Dialog::Material(name));
    } else {
        cx.fail(tr!("pick-raster-layer-register-as-material"));
    }
}

/// The material picked in the Material palette, pasted on the canvas.
fn material_paste(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let library = paint
        .material_kinds
        .scan(crate::materials::location(cx).as_deref());
    if let Some(material) = library
        .into_iter()
        .find(|material| paint.material_pick.as_deref() == Some(material.name.as_str()))
    {
        paint.paste_material(cx, &material);
    } else {
        cx.notify(tr!("pick-material-material-palette-first"));
    }
}

/// A command of this group, with its icon of the shared set: works
/// while a canvas is open unless its `state` says otherwise.
///
/// Every Edit command may be recorded in an Auto Action (as
/// `extras::recordable` has it), and none leaves the canvas: the
/// group's RECORDABLE list is all of it and its LEAVES_CANVAS list is
/// empty.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "edit", run)
        .icon(icon)
        .state(has_doc)
        .with(Flags::RECORDABLE)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("edit.undo", "undo", "undo", undo).state(can_undo),
    c("edit.redo", "redo", "redo", redo).state(can_redo),
    c("edit.cut", "cut", "cut", cut)
        .keys("Ctrl+X or F2")
        .state(on_raster),
    c("edit.copy", "copy-2", "copy", copy)
        .keys("Ctrl+C or F3")
        .state(on_raster),
    c("edit.paste", "paste", "paste", paste)
        .keys("Ctrl+V or F4")
        .state(with_clipboard),
    c(
        "edit.paste_here",
        "paste-to-shown-position",
        "paste",
        paste_here,
    )
    .state(with_clipboard),
    c("edit.clear", "delete", "clear-inside", clear)
        .keys("Ctrl+Delete or Ctrl+Backspace or Delete or Backspace")
        .state(on_raster),
    c(
        "edit.clear_outside",
        "delete-outside-selection",
        "clear-outside",
        clear_outside,
    )
    .state(with_selection),
    c(
        "edit.smart_smoothing",
        "smart-smoothing-ellipsis",
        "tool-correct-line",
        smart_smoothing,
    ),
    c("edit.fill", "fill", "fill-selection", fill)
        .keys("Shift+Delete or Shift+Backspace or Alt+Delete or Alt+Backspace")
        .state(on_raster),
    c(
        "edit.advanced_fill",
        "advanced-fill-ellipsis",
        "tool-fill",
        advanced_fill,
    ),
    c(
        "edit.recolor",
        "convert-to-drawing-color",
        "replace",
        recolor,
    )
    .state(on_raster),
    c(
        "edit.outline_selection",
        "outline-selection-ellipsis",
        "border-effect",
        outline_selection,
    ),
    c(
        "edit.luma_to_alpha",
        "convert-brightness-to-opacity",
        "extract-line",
        luma_to_alpha,
    )
    .state(on_raster),
    c(
        "edit.transform.scale",
        "scale-rotate-ellipsis",
        "transform",
        transform,
    )
    .keys("Ctrl+T"),
    c(
        "edit.transform.scale_only",
        "scale-ellipsis",
        "transform",
        transform,
    ),
    c(
        "edit.transform.rotate",
        "rotate-ellipsis",
        "rotate-right",
        transform,
    ),
    c(
        "edit.transform.free",
        "free-transform",
        "transform",
        free_transform,
    )
    .keys("Ctrl+Shift+T"),
    c(
        "edit.transform.distort",
        "distort",
        "transform",
        free_transform,
    ),
    c("edit.transform.skew", "skew", "transform", free_transform),
    c(
        "edit.transform.perspective",
        "perspective",
        "transform",
        free_transform,
    ),
    c("edit.flip_h", "flip-horizontal", "flip-horizontal", flip_h).state(on_raster),
    c("edit.flip_v", "flip-vertical", "flip-vertical", flip_v).state(on_raster),
    c(
        "edit.rotate180",
        "rotate-180-degrees",
        "rotate-right",
        rotate180,
    )
    .state(on_raster),
    c(
        "edit.transform.mesh",
        "mesh-transformation",
        "view-grid",
        mesh,
    ),
    c(
        "edit.transform.puppet",
        "puppet-warp-ellipsis",
        "pin",
        puppet,
    ),
    c("edit.align.left", "align-left", "align-left", align).state(on_raster),
    c(
        "edit.align.hcenter",
        "align-horizontal-center",
        "align-center",
        align,
    )
    .state(on_raster),
    c("edit.align.right", "align-right", "align-right", align).state(on_raster),
    c("edit.align.top", "align-top", "arrow-up", align).state(on_raster),
    c(
        "edit.align.vcenter",
        "align-vertical-center",
        "align-center",
        align,
    )
    .state(on_raster),
    c("edit.align.bottom", "align-bottom", "arrow-down", align).state(on_raster),
    c(
        "edit.resolution",
        "change-image-resolution-ellipsis",
        "picture",
        resolution,
    ),
    c(
        "edit.canvas_size",
        "change-canvas-size-ellipsis",
        "fullscreen",
        canvas_size,
    ),
    c(
        "edit.canvas_height",
        "change-canvas-height-ellipsis",
        "doc-webtoon",
        canvas_height,
    ),
    c("edit.crop", "crop", "crop", crop),
    c(
        "edit.canvas_properties",
        "canvas-properties-ellipsis",
        "info",
        canvas_properties,
    ),
    c(
        "edit.clear_history",
        "this-canvas",
        "history",
        clear_history,
    ),
    c(
        "edit.brush_tip",
        "create-brush-tip-from-image-ellipsis",
        "material-brush-tip",
        brush_tip,
    )
    .state(on_raster),
    c(
        "edit.material.image",
        "image-ellipsis",
        "material-image",
        material_image,
    ),
    c(
        "edit.material.tip",
        "brush-tip-ellipsis",
        "material-brush-tip",
        brush_tip,
    )
    .state(on_raster),
    c(
        "edit.transform.cancel",
        "cancel-transform",
        "cancel",
        cancel_transform,
    )
    .with(Flags::HIDDEN),
    c(
        "edit.material.paste",
        "paste-material",
        "materials",
        material_paste,
    )
    .with(Flags::HIDDEN),
];

#[cfg(test)]
mod tests {
    use sue_wm::Flags;
    use sue_wm::icons::Icon;

    use super::COMMANDS;

    #[test]
    fn every_edit_command_has_an_icon_of_the_shared_set() {
        for command in COMMANDS {
            assert!(
                Icon::from_name(command.icon).is_some(),
                "{}: no icon is named {:?}",
                command.name,
                command.icon
            );
        }
    }

    #[test]
    fn the_flags_say_what_auto_actions_and_the_tests_did() {
        for command in COMMANDS {
            assert_eq!(command.group, "edit");
            assert!(command.name.starts_with("edit."), "{}", command.name);
            // RECORDABLE stands where `extras::recordable` answers until
            // that function goes.
            assert_eq!(
                command.flags.has(Flags::RECORDABLE),
                crate::extras::recordable(command.name),
                "{}",
                command.name
            );
            assert!(
                !command.flags.has(Flags::LEAVES_CANVAS),
                "{} stays on the canvas",
                command.name
            );
        }
    }
}
