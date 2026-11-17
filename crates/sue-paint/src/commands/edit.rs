//! The Edit menu's commands: undo, the clipboard, fills, transforms,
//! alignment, the canvas's size and materials.
//!
//! Each command is one entry of [`COMMANDS`] with its own `run` and its
//! own `state`; commands that do the same with one difference (the six
//! alignments, the two kinds of transform) share a function and tell
//! themselves apart by their name.

use sue_core::{Arena, EntityId, Value};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::{Basis, when};
use crate::dialogs::Dialog;
use crate::doc;
use crate::filters::Filter;
use crate::select::Selection;
use crate::studio::{SuePaint, mirror};
use crate::vector::{self, Line};

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

/// Works on a vector layer.
fn on_vector(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    let Basis { info, .. } = paint.basis(facts.arena);
    when(info.is_some_and(|info| vector::is_vector(facts.arena, info.id)))
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

/// Copy vectors as SVG: the lines of the vector layer as SVG code on
/// the system clipboard, for a program that draws vectors. With lines
/// picked by the Operation tool only those go, else all of the layer.
fn copy_svg(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some((text, count)) = vectors_svg(paint, &cx.world.arena) else {
        cx.notify(tr!("no-vector-lines-to-copy"));
        return;
    };
    match sue_platform::copy_text(&text) {
        Ok(()) => cx.notify(tr!("copied-vector-lines-as-svg", count = count)),
        Err(error) => cx.fail(tr!("cannot-copy-to-clipboard", error = error)),
    }
}

/// The lines Copy vectors as SVG copies, as SVG code the size of the
/// canvas, and how many shapes it holds; `None` when there is none.
fn vectors_svg(paint: &SuePaint, arena: &Arena) -> Option<(String, usize)> {
    let info = paint.layer_info(arena)?;
    layer_svg(
        arena,
        paint.picture()?,
        info.id,
        info.style.offset,
        paint.picked_lines.as_ref(),
    )
}

/// The lines of the vector `layer` of `picture`, moved by the layer's
/// `offset`, as SVG code the size of the picture: only those `picked`
/// names when it names this layer and some of its lines. `None` when
/// `layer` is not a vector layer or nothing on it is drawn.
fn layer_svg(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
    offset: [i64; 2],
    picked: Option<&(EntityId, Vec<usize>)>,
) -> Option<(String, usize)> {
    if !vector::is_vector(arena, layer) {
        return None;
    }
    let size = arena.get_by_id(picture).ok().and_then(picture::size)?;
    let lines = vector::lines(&vector::geometry(arena, layer));
    let picked = picked
        .filter(|(on, picked)| *on == layer && !picked.is_empty())
        .map(|(_, picked)| picked.as_slice());
    svg_of(&lines, picked, offset.map(|value| value as f32), size)
}

/// `lines` (only those at the indices `picked` when given), moved by
/// `offset`, as SVG code of `size` pixels, and how many shapes it holds;
/// `None` when nothing is drawn (no line, or erasing lines only).
fn svg_of(
    lines: &[Line],
    picked: Option<&[usize]>,
    offset: [f32; 2],
    size: [u32; 2],
) -> Option<(String, usize)> {
    let chosen: Vec<Line> = lines
        .iter()
        .enumerate()
        .filter(|(index, _)| picked.is_none_or(|picked| picked.contains(index)))
        .map(|(_, line)| line.clone())
        .collect();
    let shapes = vector::svg_shapes(&chosen, offset);
    if shapes.is_empty() {
        return None;
    }
    let count = shapes.len();
    let text = sue_io::svg::write_svg(&sue_io::svg::SvgDrawing {
        width: size[0] as f32,
        height: size[1] as f32,
        shapes,
    });
    Some((text, count))
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
    paint.filter_dialog(
        cx,
        Filter::SmartSmooth {
            strength: crate::filters::SMART_LEVELS[1],
            remove_noise: false,
        },
    );
}

/// Fill: the selection (or the whole layer) in the drawing color, on
/// the layer or, while it is edited, on its mask (where the transparent
/// color hides and any other shows).
fn fill(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let target = match paint.area_target(&cx.world.arena) {
        Ok(target) => target,
        Err(error) => {
            cx.fail(error);
            return;
        }
    };
    let Some(selection) = paint.selection_or_all(&cx.world.arena) else {
        return;
    };
    let size = selection.size();
    let mask: Vec<u8> = (0..size[1])
        .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
        .map(|(x, y)| (selection.at(x, y) * 255.0).round() as u8)
        .collect();
    let label = tr!("fill");
    match crate::doc::rewrite(
        &cx.world.arena,
        target.id,
        &label,
        |pixels, width, height| match target.color {
            Some(rgb) => crate::fill::paint_mask(
                pixels,
                [width, height],
                target.offset,
                &mask,
                size,
                rgb,
                1.0,
                target.alpha_lock,
            ),
            None => crate::fill::paint_blended(
                pixels,
                [width, height],
                target.offset,
                &mask,
                size,
                None,
                1.0,
                sue_model::BlendMode::Normal,
                target.alpha_lock,
            ),
        },
    ) {
        Ok(Some(edit)) => {
            SuePaint::apply(cx, edit);
        }
        Ok(None) => cx.notify(tr!("nothing-changed", label = label)),
        Err(error) => cx.fail(error),
    }
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
fn transform(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.start_transform_in(cx, crate::transform_ui::mode_of(call.name), false);
}

/// Free Transform, Distort, Skew and Perspective: each corner on its own.
fn free_transform(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.start_transform_in(cx, crate::transform_ui::mode_of(call.name), false);
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

/// Mesh Transformation: a lattice of points over what is transformed
/// (4 by 4 at first, or as many as were chosen last), each dragged to bend
/// what is near it.
fn mesh(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.start_transform_in(cx, crate::transform::Mode::ScaleRotate, true);
    if paint.transform.is_some() {
        cx.notify(tr!("mesh-transformation-drag-points-bend-layer"));
    }
}

/// Cut and Paste: the selected part leaves the layer for a new layer in
/// the same place.
fn cut_paste(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.cut_and_paste(cx);
}

/// Copy and Paste: the selected part is copied onto a new layer in the
/// same place.
fn copy_paste(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.copy_and_paste(cx);
}

fn puppet(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.start_warp(cx);
}

/// Edit, Colorize: the line art filled from a hint layer's colors.
fn colorize(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.open_colorize(cx);
}

/// Edit, Pick Screen Color and Hide Windows and Pick Screen Color: the
/// pick starts on the next frame (it needs the window's context).
fn screen_color(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    if !sue_platform::screen::available() {
        cx.notify(tr!("pick-screen-color-unavailable"));
        return;
    }
    paint.screen_pick_asked = Some(call.name == "edit.screen_color_hidden");
}

/// Works where the system lets Sue read the screen; no canvas needed.
fn can_read_screen(_: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(sue_platform::screen::available())
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
        paint.dialog = Some(Dialog::Material(crate::dialogs::RegisterForm::new(name)));
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
/// Every Edit command may be recorded in an Auto Action, and none leaves the canvas: the
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
    c("edit.copy_svg", "copy-vectors-as-svg", "copy", copy_svg).state(on_vector),
    c("edit.paste", "paste", "paste", paste)
        .keys("Ctrl+V or F4")
        .state(with_clipboard),
    c(
        "edit.paste_here",
        "paste-to-shown-position",
        "paste",
        paste_here,
    )
    .keys("Ctrl+Shift+V")
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
    .keys("Shift+Delete or Shift+Backspace")
    .state(with_selection),
    c(
        "edit.smart_smoothing",
        "smart-smoothing-ellipsis",
        "tool-correct-line",
        smart_smoothing,
    ),
    c("edit.fill", "fill", "fill-selection", fill)
        .keys("Alt+Delete or Alt+Backspace")
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
    c("edit.cut_paste", "cut-and-paste", "cut", cut_paste).state(on_raster),
    c("edit.copy_paste", "copy-and-paste", "copy", copy_paste).state(on_raster),
    c(
        "edit.screen_color",
        "pick-screen-color-ellipsis",
        "tool-eyedropper",
        screen_color,
    )
    .state(can_read_screen),
    c(
        "edit.screen_color_hidden",
        "hide-windows-and-pick-screen-color-ellipsis",
        "tool-eyedropper",
        screen_color,
    )
    .state(can_read_screen),
    c("edit.colorize", "colorize-ellipsis", "tool-fill", colorize).state(on_raster),
];

#[cfg(test)]
mod tests {
    use sue_wm::Flags;
    use sue_wm::icons::Icon;

    use super::{COMMANDS, svg_of};
    use crate::brush::{Finish, SAME, Sample, Tip};
    use crate::vector::Line;

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
            assert!(command.flags.has(Flags::RECORDABLE), "{}", command.name);
            assert!(
                !command.flags.has(Flags::LEAVES_CANVAS),
                "{} stays on the canvas",
                command.name
            );
        }
    }

    /// A line of 21 samples from `from` to `to`, 4 pixels wide.
    fn line(from: [f32; 2], to: [f32; 2]) -> Line {
        let samples = (0..=20)
            .map(|step| {
                let t = step as f32 / 20.0;
                Sample {
                    at: [
                        from[0] + (to[0] - from[0]) * t,
                        from[1] + (to[1] - from[1]) * t,
                    ],
                    pressure: 1.0,
                    speed: 0.2,
                    tilt: 0.0,
                    lean: [0.0, 0.0],
                    width: 1.0,
                    alpha: 1.0,
                }
            })
            .collect();
        Line {
            samples,
            tip: Tip {
                size: 4.0,
                ..Tip::default()
            },
            colors: [[0.0; 3], [1.0; 3]],
            finish: Finish::default(),
            copies: vec![SAME],
            marks: Vec::new(),
            engine: None,
        }
    }

    #[test]
    fn copy_vectors_as_svg_writes_the_lines_as_svg_code() {
        let mut red = line([10.0, 10.0], [90.0, 10.0]);
        red.colors[0] = [1.0, 0.0, 0.0];
        let blue = {
            let mut blue = line([10.0, 50.0], [90.0, 80.0]);
            blue.colors[0] = [0.0, 0.0, 1.0];
            blue
        };
        let mut eraser = line([0.0, 0.0], [20.0, 20.0]);
        eraser.tip.eraser = true;
        let lines = [red, blue, eraser];
        // Every drawn line, as big as the canvas, moved with its layer.
        let (text, count) = svg_of(&lines, None, [5.0, 0.0], [200, 100]).expect("svg");
        assert_eq!(count, 2, "an erasing line draws nothing");
        let drawing = sue_io::svg::parse_svg(&text).expect("it reads back");
        assert_eq!((drawing.width, drawing.height), (200.0, 100.0));
        assert_eq!(drawing.shapes.len(), 2);
        assert!(
            text.contains("#ff0000") && text.contains("#0000ff"),
            "{text}"
        );
        let left = drawing
            .shapes
            .iter()
            .flat_map(|shape| shape.points.iter())
            .map(|point| point[0])
            .fold(f32::MAX, f32::min);
        assert!(left >= 13.0, "moved by the layer's offset: {left}");
        // Only the picked lines.
        let (text, count) = svg_of(&lines, Some(&[1]), [0.0, 0.0], [200, 100]).expect("svg");
        assert_eq!(count, 1);
        assert!(text.contains("#0000ff") && !text.contains("#ff0000"));
        // Nothing to copy: no line, or only an erasing one.
        assert!(svg_of(&[], None, [0.0, 0.0], [200, 100]).is_none());
        assert!(svg_of(&lines, Some(&[2]), [0.0, 0.0], [200, 100]).is_none());
    }

    #[test]
    fn copy_vectors_as_svg_reads_the_layer_from_the_arena() {
        use sue_core::{Arena, ReversibleTransaction};

        use crate::doc;

        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&doc::NewPicture {
            width: 200,
            height: 100,
            ..doc::NewPicture::default()
        })
        .expect("a picture");
        edit.apply(&mut arena).expect("made");
        let raster = doc::layers(&arena, picture)
            .last()
            .map(|info| info.id)
            .expect("a raster layer");
        let lines = [
            line([10.0, 10.0], [90.0, 10.0]),
            line([10.0, 50.0], [90.0, 80.0]),
        ];
        let (mut spawn, ink) =
            doc::add_vector_lines(&arena, picture, Some(raster), "Ink".to_owned(), &lines)
                .expect("a vector layer");
        spawn.apply(&mut arena).expect("added");

        // A raster layer has no lines to copy.
        assert!(super::layer_svg(&arena, picture, raster, [0, 0], None).is_none());
        // Every line, as big as the picture, moved with the layer.
        let (text, count) =
            super::layer_svg(&arena, picture, ink, [30, 0], None).expect("the lines");
        assert_eq!(count, 2);
        let drawing = sue_io::svg::parse_svg(&text).expect("it reads back");
        assert_eq!((drawing.width, drawing.height), (200.0, 100.0));
        let left = drawing
            .shapes
            .iter()
            .flat_map(|shape| shape.points.iter())
            .map(|point| point[0])
            .fold(f32::MAX, f32::min);
        assert!((37.0..40.0).contains(&left), "moved by 30: {left}");
        // Lines picked on this layer narrow it; picked on another do not.
        let picked = (ink, vec![0]);
        let (_, count) =
            super::layer_svg(&arena, picture, ink, [0, 0], Some(&picked)).expect("one");
        assert_eq!(count, 1);
        let elsewhere = (raster, vec![0]);
        let (_, count) =
            super::layer_svg(&arena, picture, ink, [0, 0], Some(&elsewhere)).expect("all");
        assert_eq!(count, 2);
        let none_picked = (ink, Vec::new());
        let (_, count) =
            super::layer_svg(&arena, picture, ink, [0, 0], Some(&none_picked)).expect("all");
        assert_eq!(count, 2);
    }
}
