//! The Layer menu's commands: one entry each, with what it does and how
//! its item looks.

use sue_core::txn::SetProperty;
use sue_core::{Arena, EntityId, ReversibleTransaction};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::{doc_open, flag, when};
use crate::dialogs::{ConvertDialog, Dialog, PerspectiveDialog};
use crate::doc;
use crate::filters::Filter;
use crate::select::{Op, Selection};
use crate::studio::SuePaint;

// How the items look.

/// The current layer is a raster layer.
fn raster(paint: &SuePaint, facts: &Facts<'_>) -> bool {
    paint
        .layer_info(facts.arena)
        .is_some_and(|info| info.kind == LayerKind::Raster)
}

/// The canvas has a selection.
fn selection(paint: &SuePaint) -> bool {
    paint.doc().is_some_and(|doc| doc.selection.is_some())
}

/// The current layer has a mask.
fn masked(paint: &SuePaint, facts: &Facts<'_>) -> bool {
    paint
        .layer_info(facts.arena)
        .is_some_and(|info| doc::mask_of(facts.arena, info.id).is_some())
}

/// Works on a raster layer.
fn on_raster(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(raster(paint, facts))
}

fn overlap_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(selection(paint) && raster(paint, facts))
}

/// Merge Selected Layers: two or more picked layers of any kind, or a
/// folder alone (it is baked into one layer), or a layer alone in a
/// folder cel (the cel is merged).
fn merge_selected_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    let folder = paint
        .layer()
        .is_some_and(|layer| doc::is_folder(facts.arena, layer));
    when(
        paint.doc().is_some_and(|doc| !doc.picked.is_empty())
            || folder
            || folder_cel_around(paint, facts.arena).is_some(),
    )
}

/// The folder cel the current layer lies in, when it is the only layer
/// picked: going to a frame that shows a folder cel picks a layer in it
/// (TR-1), and Merge Selected Layers then merges that cel, as an auto
/// action that merges each cel and steps on expects (the 2025 tutorial,
/// V26).
fn folder_cel_around(paint: &SuePaint, arena: &Arena) -> Option<EntityId> {
    let doc = paint.doc()?;
    if !doc.picked.is_empty() {
        return None;
    }
    let layer = doc.layer?;
    if doc::is_folder(arena, layer) {
        return None;
    }
    let cel = doc::folder_of(arena, layer)?;
    crate::anim::cel_folder(arena, cel).map(|_| cel)
}

fn from_selection_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(selection(paint) && raster(paint, facts))
}

/// Merge with Layer Below: any layer but the paper, a folder with what
/// it holds, onto a raster layer right under it (under all a folder
/// holds).
fn merge_down_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    let arena = facts.arena;
    let picture = paint.picture().unwrap_or_default();
    let below_raster = paint.layer_info(arena).is_some_and(|info| {
        if info.kind == LayerKind::Paper {
            return false;
        }
        let stack = doc::layers(arena, picture);
        let block = doc::with_members(arena, picture, &[info.id]);
        stack
            .iter()
            .position(|layer| block.contains(&layer.id))
            .and_then(|at| at.checked_sub(1))
            .is_some_and(|below| {
                stack[below].kind == LayerKind::Raster && !doc::is_folder(arena, stack[below].id)
            })
    });
    when(below_raster)
}

fn new_paper_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint.doc().is_some()
            && !doc::layers(facts.arena, paint.picture().unwrap_or_default())
                .iter()
                .any(|layer| layer.kind == LayerKind::Paper),
    )
}

fn visible_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint
        .layer_info(facts.arena)
        .map_or(ItemState::off(), |info| flag(info.style.visible))
}

fn clip_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint
        .layer_info(facts.arena)
        .map_or(ItemState::off(), |info| flag(info.style.clip))
}

fn draft_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint
        .layer_info(facts.arena)
        .map_or(ItemState::off(), |info| flag(info.style.draft))
}

fn lock_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint
        .layer_info(facts.arena)
        .map_or(ItemState::off(), |info| flag(info.locked))
}

/// Checked while the current layer is shown in its layer color.
fn layer_color_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint.layer().map_or(ItemState::off(), |layer| {
        flag(
            facts
                .arena
                .get_by_id(layer)
                .ok()
                .and_then(|entity| entity.property(crate::effects::COLOR))
                .and_then(sue_core::Value::as_bool)
                .unwrap_or(false),
        )
    })
}

fn reference_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint
        .layer_info(facts.arena)
        .map_or(ItemState::off(), |info| {
            flag(doc::is_reference(facts.arena, info.id))
        })
}

/// Rasterize: anything Rasterize changes (a folder, a vector, text, 3D
/// or frame layer, a raster layer with effects on).
fn rasterize_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.layer().is_some_and(|layer| {
        doc::rasterizes(facts.arena, layer) || crate::vector::is_vector(facts.arena, layer)
    }))
}

/// Mask Outside Selection works on a raster layer without a mask; Mask
/// Selection needs a selection too. Either is checked while there is a
/// mask.
fn new_mask_state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    let masked = masked(paint, facts);
    // Any layer takes a mask but the paper: a raster layer, a folder, a
    // correction layer.
    let maskable = paint
        .layer_info(facts.arena)
        .is_some_and(|info| info.kind != LayerKind::Paper);
    let mut state =
        when(maskable && !masked && (call.name == "layer.mask.outside" || selection(paint)));
    state.checked = Some(masked);
    state
}

fn has_mask_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(masked(paint, facts))
}

fn mask_on_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    match paint
        .layer_info(facts.arena)
        .filter(|info| doc::mask_of(facts.arena, info.id).is_some())
    {
        Some(info) => flag(doc::mask_on(facts.arena, info.id)),
        None => ItemState::off(),
    }
}

fn mask_edit_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    if masked(paint, facts) {
        flag(paint.doc().is_some_and(|doc| doc.mask_edit))
    } else {
        ItemState::off()
    }
}

fn show_rulers_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint
        .doc()
        .map_or(ItemState::off(), |doc| flag(doc.show_rulers))
}

fn delete_ruler_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint
            .doc()
            .is_some_and(|doc| !crate::rulers::Rulers::of(facts.arena, doc.picture).is_empty()),
    )
}

// What they do.

/// Makes `id` the current layer once `edit` went in.
fn apply_and_pick(
    paint: &mut SuePaint,
    cx: &mut AppletContext<'_>,
    edit: impl ReversibleTransaction<Arena> + 'static,
    id: EntityId,
) {
    if SuePaint::apply(cx, edit)
        && let Some(doc) = paint.doc_mut()
    {
        doc.layer = Some(id);
    }
}

/// New Layer from Selection: what is selected, copied onto a layer of
/// its own; the clipboard keeps what it held.
fn from_selection(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let kept = paint.clipboard.take();
    if paint.copy(cx) {
        paint.paste(cx, &tr!("from-selection"));
    }
    paint.clipboard = kept;
}

/// Clip Studio Paint's Convert to lines and tones with the lines as a
/// vector layer: a 3D layer's outline and folds as strokes.
fn lines_tones_vector(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    match paint
        .layer()
        .filter(|layer| crate::three::is_3d(&cx.world.arena, *layer))
    {
        Some(layer) => paint.lines_and_tones_3d(cx, layer, true),
        None => cx.notify(tr!("pick-a-3d-layer-first")),
    }
}

fn new_raster_top(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.run("layer.new", cx);
}

/// New Layer, Tone: Simple Tone Settings, then a new layer of tone in
/// the selection if there is one.
fn new_tone(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.picture().is_some() {
        paint.dialog = Some(Dialog::SimpleTone(crate::panels::SimpleTone::default()));
    }
}

fn new_frame(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Frame(crate::comic::FrameSpec::default()));
}

/// Divide Frame Border Equally: asks how to cut the panel picked.
fn divide_frame(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.ask_divide_frame(cx);
}

/// Combine Frames: the panels picked become one.
fn combine_frames(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.combine_frames(cx);
}

/// Divide Frame Border Equally works with one panel picked on a frame
/// border layer, Combine Frames with two or more.
fn frame_picked_state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    let picked = paint
        .picked_panels(facts.arena)
        .map_or(0, |(_, picked)| picked.len());
    when(if call.name == "layer.frame.combine" {
        picked >= 2
    } else {
        picked == 1
    })
}

fn new_balloon(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.add_balloon_layer(cx);
}

/// A correction layer of `filter` over the current layer, then its
/// settings (Reverse Gradient has none).
fn correction(paint: &mut SuePaint, cx: &mut AppletContext<'_>, filter: Filter) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let text = filter.to_text().unwrap_or_default();
    let (edit, layer) = doc::new_correction(
        &cx.world.arena,
        picture,
        paint.layer(),
        &filter.label(),
        &text,
    );
    if SuePaint::apply(cx, edit) {
        if let Some(doc) = paint.doc_mut() {
            doc.layer = Some(layer);
        }
        if filter != Filter::Invert {
            paint.dialog = Some(Dialog::Correction(layer, filter, text, true));
        }
    }
}

fn correction_brightness(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let filter = Filter::BrightnessContrast {
        brightness: 0.0,
        contrast: 0.0,
    };
    correction(paint, cx, filter);
}

fn correction_levels(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let filter = Filter::Levels {
        black: 0.0,
        white: 1.0,
        gamma: 1.0,
        out_black: 0.0,
        out_white: 1.0,
    };
    correction(paint, cx, filter);
}

fn correction_curve(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let filter = Filter::ToneCurve {
        rgb: sue_model::Response::LINEAR,
        red: sue_model::Response::LINEAR,
        green: sue_model::Response::LINEAR,
        blue: sue_model::Response::LINEAR,
        channel: 0,
    };
    correction(paint, cx, filter);
}

fn correction_hue(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let filter = Filter::HueSaturation {
        hue: 0.0,
        saturation: 0.0,
        luminosity: 0.0,
    };
    correction(paint, cx, filter);
}

fn correction_balance(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let filter = Filter::ColorBalance {
        shifts: [[0.0; 3]; 3],
        range: 1,
    };
    correction(paint, cx, filter);
}

fn correction_invert(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    correction(paint, cx, Filter::Invert);
}

fn correction_posterize(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    correction(paint, cx, Filter::Posterize { levels: 4 });
}

fn correction_binarize(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    correction(paint, cx, Filter::Binarize { threshold: 0.5 });
}

fn correction_gradient_map(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let filter = Filter::GradientMap {
        from: paint.main,
        to: paint.sub,
    };
    correction(paint, cx, filter);
}

/// Text: the text dialog, for words in the middle of the canvas.
fn new_text(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let middle = paint
        .picture()
        .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
        .and_then(picture::size)
        .map_or([0.0, 0.0], |[w, h]| [w as f32 / 2.0, h as f32 / 2.0]);
    let mut lettering = crate::lettering::Lettering::new(middle, paint.drawing_color());
    lettering.center = true;
    paint.dialog = Some(Dialog::Text(None, lettering));
}

fn show_rulers(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(doc) = paint.doc_mut() {
        doc.show_rulers = !doc.show_rulers;
    }
}

fn perspective_ruler(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.picture().is_some() {
        paint.dialog = Some(Dialog::Perspective(PerspectiveDialog::new(2)));
    }
}

/// A symmetrical ruler or a concentric circle ruler in the middle of the
/// canvas, picked and shown.
fn middle_ruler(paint: &mut SuePaint, cx: &mut AppletContext<'_>, symmetry: bool) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let Some(size) = cx
        .world
        .arena
        .get_by_id(picture)
        .ok()
        .and_then(picture::size)
    else {
        return;
    };
    let middle = [size[0] as f32 / 2.0, size[1] as f32 / 2.0];
    let (key, text, label, mode) = if symmetry {
        let rulers = crate::rulers::Rulers {
            symmetry: Some((middle, std::f32::consts::FRAC_PI_2)),
            ..crate::rulers::Rulers::default()
        };
        (
            crate::rulers::SYMMETRY,
            rulers.text(crate::rulers::SYMMETRY),
            tr!("create-symmetrical-ruler"),
            0,
        )
    } else {
        let special = crate::rulers::Special::Concentric(middle);
        (
            crate::rulers::SPECIAL,
            Some(special.to_text()),
            tr!("create-concentric-circle-ruler"),
            2,
        )
    };
    if SuePaint::apply(cx, crate::rulers::edit(picture, key, text, &label)) {
        paint.pick_ruler(mode);
        if mode == 2 {
            paint.toolbox.current_mut().kind = 2;
        }
        if let Some(doc) = paint.doc_mut() {
            doc.ruler_on = true;
            doc.show_rulers = true;
        }
    }
}

fn symmetry_ruler(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    middle_ruler(paint, cx, true);
}

fn circle_ruler(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    middle_ruler(paint, cx, false);
}

fn delete_ruler(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let rulers = crate::rulers::Rulers::of(&cx.world.arena, picture);
    let mut edit = doc::Edit::new(tr!("delete-ruler"));
    for key in crate::rulers::ALL {
        if rulers.text(key).is_some() {
            edit = edit.then(SetProperty::remove(picture, key));
        }
    }
    paint.vanishing_line = None;
    if !edit.is_empty() {
        SuePaint::apply(cx, edit);
    }
}

/// A new raster layer over the current one: empty, or filled with the
/// drawing color.
fn new_raster(paint: &mut SuePaint, cx: &mut AppletContext<'_>, filled: bool) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let arena = &cx.world.arena;
    let name = if filled {
        tr!("fill")
    } else {
        doc::new_layer_name(arena, picture)
    };
    let fill = filled.then(|| {
        let rgb = paint.drawing_color();
        [rgb[0], rgb[1], rgb[2], 255]
    });
    match doc::add_raster_layer(arena, picture, paint.layer(), name, fill) {
        Ok((spawn, id)) => apply_and_pick(paint, cx, spawn, id),
        Err(error) => cx.fail(error),
    }
}

fn new_layer(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    new_raster(paint, cx, false);
}

fn new_fill(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    new_raster(paint, cx, true);
}

fn new_vector(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let name = doc::new_layer_name(&cx.world.arena, picture);
    match doc::add_vector_layer(&cx.world.arena, picture, paint.layer(), name) {
        Ok((spawn, id)) => apply_and_pick(paint, cx, spawn, id),
        Err(error) => cx.fail(error),
    }
}

fn new_paper(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(picture) = paint.picture() {
        let (spawn, _) = doc::add_paper(
            &cx.world.arena,
            picture,
            sue_core::Color::srgb8(255, 255, 255),
        );
        SuePaint::apply(cx, spawn);
    }
}

fn duplicate(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let (Some(picture), Some(layer)) = (paint.picture(), paint.layer()) {
        match doc::duplicate(&cx.world.arena, picture, layer) {
            Ok((spawn, id)) => apply_and_pick(paint, cx, spawn, id),
            Err(error) => cx.fail(error),
        }
    }
}

/// Deletes the current layer and every picked one, a folder with what it
/// holds (asking first for folders, see `SuePaint::delete_layers`).
fn delete(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.delete_layers(cx, false);
}

fn toggle_visible(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.toggle_layer(cx, picture::VISIBLE, true);
}

fn comps(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.picture().is_some() {
        paint.dialog = Some(Dialog::Comps(crate::dialogs::CompsDialog::default()));
    }
}

fn new_3d(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.picture().is_some() {
        paint.dialog = Some(Dialog::Shape3D(0));
    }
}

/// Clip Studio Paint's 3D drawing figure, standing, to pose.
fn new_figure(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let name = tr!("drawing-figure");
    paint.add_3d_layer(cx, &name, crate::figure::new_model(&name));
}

/// Clip Studio Paint 5.0's 3D hand, to pose.
fn new_hand(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let name = tr!("n-3d-hand");
    paint.add_3d_layer(
        cx,
        &name,
        crate::figure::new_of(&name, crate::figure::Kind::Hand),
    );
}

/// Clip Studio Paint 5.1's 3D head, to pose.
fn new_head(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let name = tr!("n-3d-head");
    paint.add_3d_layer(
        cx,
        &name,
        crate::figure::new_of(&name, crate::figure::Kind::Head),
    );
}

fn rasterize(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let (Some(picture), Some(layer)) = (paint.picture(), paint.layer()) else {
        return;
    };
    match doc::rasterize(&cx.world.arena, picture, layer) {
        Ok((edit, id)) => apply_and_pick(paint, cx, edit, id),
        Err(error) => cx.fail(error),
    }
}

fn toggle_reference(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(layer) = paint.layer() {
        let on = doc::is_reference(&cx.world.arena, layer);
        SuePaint::apply(cx, doc::set(layer, doc::REFERENCE, !on));
    }
}

fn toggle_clip(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.toggle_layer(cx, picture::CLIP, false);
}

/// A mask on the current layer that shows what is selected (or all of
/// it, with no selection), or, `inside`, hides what is selected.
fn new_mask(paint: &mut SuePaint, cx: &mut AppletContext<'_>, inside: bool) {
    let Some(layer) = paint.layer() else {
        return;
    };
    let selection = paint.doc().and_then(|doc| doc.selection.clone());
    if inside && selection.is_none() {
        cx.fail(tr!("select-what-to-hide-first"));
        return;
    }
    let show = |x: i64, y: i64| {
        let cover = selection.as_ref().map_or(1.0, |selection| {
            if x < 0 || y < 0 {
                0.0
            } else {
                selection.at(x as u32, y as u32)
            }
        });
        if inside { 1.0 - cover } else { cover }
    };
    match doc::new_mask(&cx.world.arena, layer, show) {
        Ok((edit, _)) => {
            SuePaint::apply(cx, edit);
            // As in Clip Studio Paint, the new mask is what the tools
            // draw on next: transparent hides, a color shows.
            if let Some(doc) = paint.doc_mut() {
                doc.mask_edit = true;
            }
        }
        Err(error) => cx.fail(error),
    }
}

fn mask_outside(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    new_mask(paint, cx, false);
}

fn mask_inside(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    new_mask(paint, cx, true);
}

fn mask_delete(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(edit) = paint
        .layer()
        .and_then(|layer| doc::delete_mask(&cx.world.arena, layer))
    {
        SuePaint::apply(cx, edit);
        if let Some(doc) = paint.doc_mut() {
            doc.mask_edit = false;
        }
    }
}

fn mask_apply(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(layer) = paint.layer() {
        match doc::apply_mask(&cx.world.arena, layer) {
            Ok(edit) => {
                SuePaint::apply(cx, edit);
                if let Some(doc) = paint.doc_mut() {
                    doc.mask_edit = false;
                }
            }
            Err(error) => cx.fail(error),
        }
    }
}

fn mask_toggle(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(layer) = paint.layer() {
        let on = doc::mask_on(&cx.world.arena, layer);
        SuePaint::apply(cx, doc::set(layer, doc::MASK_ON, !on));
    }
}

fn mask_edit(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let masked = paint
        .layer()
        .is_some_and(|layer| doc::mask_of(&cx.world.arena, layer).is_some());
    if let Some(doc) = paint.doc_mut() {
        doc.mask_edit = masked && !doc.mask_edit;
    }
}

fn toggle_draft(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.toggle_layer(cx, picture::DRAFT, false);
}

fn toggle_lock(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.toggle_layer(cx, picture::LOCKED, false);
}

fn toggle_lock_alpha(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.toggle_layer(cx, picture::ALPHA_LOCKED, false);
}

/// Switch Using Layer Color: the current layer drawn in its layer color
/// (blue unless Layer Property says otherwise), or in its own colors
/// again, as the Layer color button of Layer Property does.
fn toggle_layer_color(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.toggle_layer(cx, crate::effects::COLOR, false);
}

/// Layer Property: the palette comes forward.
fn properties(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.layer = true;
    // A phone's sheet shows it as the Layer palette's tab; the dock has
    // Layer Property as a palette of its own, which comes forward.
    paint.layer_tab = 1;
    paint.show_palette(crate::dock::Palette::LayerProperty);
}

/// Clip Studio Paint's Select all layers: every layer picked with the
/// current one (for merging or correcting them all); or none.
fn pick_layers(paint: &mut SuePaint, cx: &mut AppletContext<'_>, all: bool) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let current = paint.layer();
    let stack = if all {
        doc::layers(&cx.world.arena, picture)
    } else {
        Vec::new()
    };
    let picked: Vec<EntityId> = stack
        .into_iter()
        .map(|info| info.id)
        .filter(|id| Some(*id) != current)
        .collect();
    if let Some(doc) = paint.doc_mut() {
        doc.picked = picked;
    }
}

fn select_all(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    pick_layers(paint, cx, true);
}

fn select_none(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    pick_layers(paint, cx, false);
}

/// The layer above (or below) the current one becomes current; with no
/// current layer, the top one.
fn select_next(paint: &mut SuePaint, cx: &mut AppletContext<'_>, above: bool) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let stack = doc::layers(&cx.world.arena, picture);
    let at = paint
        .layer()
        .and_then(|layer| stack.iter().position(|info| info.id == layer));
    let next = match (at, above) {
        (Some(at), true) => stack.get(at + 1),
        (Some(at), false) => at.checked_sub(1).and_then(|at| stack.get(at)),
        (None, _) => stack.last(),
    };
    if let (Some(next), Some(doc)) = (next.map(|info| info.id), paint.doc_mut()) {
        doc.layer = Some(next);
    }
}

fn select_above(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    select_next(paint, cx, true);
}

fn select_below(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    select_next(paint, cx, false);
}

/// A new folder over the current layer; `insert` puts the layer in it.
fn folder(paint: &mut SuePaint, cx: &mut AppletContext<'_>, insert: bool) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let layer = paint
        .layer()
        .filter(|layer| !doc::is_folder(&cx.world.arena, *layer));
    let (mut edit, folder) =
        doc::new_folder(&cx.world.arena, picture, layer.or(paint.layer()), insert);
    // A cel wrapped in a folder: the folder becomes the cel (TR-1).
    if insert
        && let Some(wrap) =
            layer.and_then(|cel| crate::anim::cels::wrap_cel(&cx.world.arena, picture, cel, folder))
    {
        edit = edit.then(wrap);
    }
    apply_and_pick(paint, cx, edit, folder);
}

/// Create Folder and Insert Layer: every picked layer goes into one new
/// folder, in their order (one layer alone as before).
fn folder_insert(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let picked = paint.picked_layers(&cx.world.arena);
    if picked.len() > 1
        && let Some(picture) = paint.picture()
        && let Some((edit, folder)) = doc::folder_insert(&cx.world.arena, picture, &picked)
    {
        apply_and_pick(paint, cx, edit, folder);
        if let Some(doc) = paint.doc_mut() {
            doc.picked.clear();
        }
        return;
    }
    folder(paint, cx, true);
}

fn new_folder(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    folder(paint, cx, false);
}

/// What a layer whose pixels start at `offset` covers on a canvas of
/// `size`: one byte per canvas pixel, its alpha where it lies and 0
/// elsewhere.
fn on_canvas(alpha: &Selection, [ox, oy]: [i64; 2], [width, height]: [u32; 2]) -> Vec<u8> {
    let [layer_width, layer_height] = alpha.size();
    let mut drawn = vec![0_u8; width as usize * height as usize];
    for y in 0..layer_height {
        for x in 0..layer_width {
            let (px, py) = (i64::from(x) + ox, i64::from(y) + oy);
            if px >= 0 && py >= 0 && px < i64::from(width) && py < i64::from(height) {
                drawn[py as usize * width as usize + px as usize] =
                    alpha.mask()[y as usize * layer_width as usize + x as usize];
            }
        }
    }
    drawn
}

/// Select Overlapping Area (Selection from Layer): the selection keeps
/// only where the current layer has something drawn, as much as it is
/// drawn there. The layer's pixels are read where the layer lies on the
/// canvas.
fn select_overlapping(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let arena = &cx.world.arena;
    let Some(info) = paint.layer_info(arena) else {
        return;
    };
    let Some([width, height]) = paint
        .picture()
        .and_then(|picture| arena.get_by_id(picture).ok())
        .and_then(picture::size)
    else {
        return;
    };
    let Some(raster) = doc::raster(arena, info.id) else {
        return;
    };
    let drawn = on_canvas(
        &Selection::from_alpha(raster),
        info.style.offset,
        [width, height],
    );
    if let Some(doc) = paint.doc_mut()
        && let Some(mut selection) = doc.selection.clone()
        && selection.size() == [width, height]
    {
        selection.apply_mask(&drawn, Op::Intersect);
        doc.set_selection(Some(selection));
    }
}

/// Where Move Up, Move Down, Bring to Front and Send to Back take the
/// current layer.
#[derive(Clone, Copy)]
enum Order {
    Front,
    Up,
    Down,
    Back,
}

fn reorder(paint: &mut SuePaint, cx: &mut AppletContext<'_>, order: Order) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let stack = doc::layers(&cx.world.arena, picture);
    let Some(at) = paint
        .layer()
        .and_then(|layer| stack.iter().position(|info| info.id == layer))
    else {
        return;
    };
    let last = stack.len().saturating_sub(1);
    let to = match order {
        Order::Front => last,
        Order::Up => (at + 1).min(last),
        Order::Down => at.saturating_sub(1),
        Order::Back => 0,
    };
    paint.move_layer_to(cx, to);
}

fn order_front(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    reorder(paint, cx, Order::Front);
}

/// One up; into or out of a folder first where there is one.
fn order_up(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if !paint.move_in_folders(cx, true) {
        reorder(paint, cx, Order::Up);
    }
}

/// One down; into or out of a folder first where there is one.
fn order_down(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if !paint.move_in_folders(cx, false) {
        reorder(paint, cx, Order::Down);
    }
}

fn order_back(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    reorder(paint, cx, Order::Back);
}

fn merge_down(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let (Some(picture), Some(layer)) = (paint.picture(), paint.layer()) {
        match doc::merge_down(&cx.world.arena, picture, layer) {
            Ok((edit, into)) => apply_and_pick(paint, cx, edit, into),
            Err(error) => cx.fail(error),
        }
    }
}

/// Ungroup Layer Folder: its layers go where it was; then it goes, and
/// an animation folder's track with it.
fn ungroup(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let (Some(picture), Some(folder)) = (paint.picture(), paint.layer()) else {
        return;
    };
    let arena = &cx.world.arena;
    if !doc::is_folder(arena, folder) {
        cx.fail(tr!("pick-a-layer-folder-first"));
        return;
    }
    let outer = doc::folder_of(arena, folder);
    let mut edit = doc::Edit::new(tr!("ungroup-layer-folder"));
    for layer in doc::layers(arena, picture) {
        if doc::folder_of(arena, layer.id) == Some(folder)
            && let Some(moved) = doc::set_folder(arena, layer.id, outer)
        {
            edit = edit.then(moved);
        }
    }
    if let Some(sequence) = crate::anim::timeline(arena, picture)
        && let Some(track) = crate::anim::track_of(sequence, folder)
    {
        edit = edit.then(sue_core::txn::SetExtension::set(
            picture,
            crate::anim::without_track(sequence, track),
        ));
    }
    edit = edit.then(sue_core::txn::DespawnEntity::new(folder));
    if SuePaint::apply(cx, edit)
        && let Some(doc) = paint.doc_mut()
    {
        doc.layer = None;
    }
}

/// Deletes the raster layers that are empty, or (`hidden`) the hidden
/// ones; one layer always stays.
fn delete_some(paint: &mut SuePaint, cx: &mut AppletContext<'_>, hidden: bool) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let arena = &cx.world.arena;
    let stack = doc::layers(arena, picture);
    let gone: Vec<EntityId> = stack
        .iter()
        .filter(|info| info.kind == LayerKind::Raster && !doc::is_folder(arena, info.id))
        .filter(|info| {
            if hidden {
                !info.style.visible
            } else {
                doc::raster(arena, info.id).is_some_and(|raster| {
                    raster
                        .bytes()
                        .is_some_and(|bytes| bytes.iter().skip(3).step_by(4).all(|a| *a == 0))
                })
            }
        })
        .map(|info| info.id)
        .collect();
    if gone.is_empty() {
        cx.notify(tr!("no-such-layers"));
        return;
    }
    if gone.len() >= stack.len() {
        cx.fail(tr!("picture-keeps-least-one-layer"));
        return;
    }
    let mut edit = doc::Edit::new(if hidden {
        tr!("hidden-layers")
    } else {
        tr!("empty-layers")
    });
    for id in gone {
        edit = edit.then(sue_core::txn::DespawnEntity::new(id));
    }
    if SuePaint::apply(cx, edit)
        && let Some(doc) = paint.doc_mut()
    {
        doc.layer = None;
    }
}

fn delete_empty(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    delete_some(paint, cx, false);
}

fn delete_hidden(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    delete_some(paint, cx, true);
}

fn hide_drafts(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let mut edit = doc::Edit::new(tr!("hide-all-visible-draft-layers"));
    for info in doc::layers(&cx.world.arena, picture) {
        if info.style.draft && info.style.visible {
            edit = edit.then(SetProperty::new(info.id, picture::VISIBLE, false));
        }
    }
    if !edit.is_empty() {
        SuePaint::apply(cx, edit);
    }
}

/// The current layer's name, ready to be typed over in the Layer
/// palette.
fn rename(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(info) = paint.layer_info(&cx.world.arena) {
        paint.panels.layer = true;
        paint.layer_tab = 0;
        paint.renaming = Some((info.id, info.name));
    }
}

/// Merge Visible Layers: the visible layers become one, the hidden ones
/// stay.
fn merge_visible_here(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let arena = &cx.world.arena;
    match doc::merge_visible(arena, picture, false) {
        Ok((merged, id)) => {
            let mut edit = doc::Edit::new(tr!("merge-visible-layers")).then(merged);
            for info in doc::layers(arena, picture) {
                if info.style.visible && !doc::is_folder(arena, info.id) {
                    edit = edit.then(sue_core::txn::DespawnEntity::new(info.id));
                }
            }
            apply_and_pick(paint, cx, edit, id);
        }
        Err(error) => cx.fail(error),
    }
}

fn transfer_down(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let (Some(picture), Some(layer)) = (paint.picture(), paint.layer()) {
        match doc::transfer_down(&cx.world.arena, picture, layer) {
            Ok((edit, _)) => {
                SuePaint::apply(cx, edit);
            }
            Err(error) => cx.fail(error),
        }
    }
}

/// The visible layers merged onto a new layer, or (`flatten`) every
/// layer merged into one.
fn merge_all(paint: &mut SuePaint, cx: &mut AppletContext<'_>, flatten: bool) {
    if let Some(picture) = paint.picture() {
        match doc::merge_visible(&cx.world.arena, picture, flatten) {
            Ok((edit, id)) => apply_and_pick(paint, cx, edit, id),
            Err(error) => cx.fail(error),
        }
    }
}

fn merge_visible(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    merge_all(paint, cx, false);
}

fn flatten(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    merge_all(paint, cx, true);
}

/// Layer, New Layer, Gradient: a gradient layer over the whole canvas,
/// the main color at the top fading into the sub color at the bottom.
/// It stays a gradient: the Gradient or Object tool changes its line,
/// stops, shape and edge later.
fn new_gradient(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let arena = &cx.world.arena;
    let Some([width, height]) = arena.get_by_id(picture).ok().and_then(picture::size) else {
        return;
    };
    let middle = width as f32 / 2.0;
    let gradient = doc::Gradient {
        line: [[middle, 0.0], [middle, height as f32]],
        look: crate::gradient::Look {
            map: crate::gradient_map::GradientMap::two(paint.main, paint.sub),
            ..crate::gradient::Look::default()
        },
    };
    let (spawn, id) =
        doc::new_gradient_layer(arena, picture, paint.layer(), &tr!("gradient"), gradient);
    let edit = doc::Edit::new(tr!("new-gradient-layer")).then(spawn);
    apply_and_pick(paint, cx, edit, id);
}

/// The picked layers and the current one merged into one.
fn merge_selected(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let (Some(picture), Some(doc)) = (paint.picture(), paint.doc()) else {
        return;
    };
    let mut ids = doc.picked.clone();
    ids.extend(folder_cel_around(paint, &cx.world.arena).or(doc.layer));
    match doc::merge_layers(&cx.world.arena, picture, &ids) {
        Ok((edit, into)) => {
            // A folder cel merged stays the cel: its frames show the layer
            // it became.
            let edit = match crate::anim::cels::rekeyed(&cx.world.arena, picture, &ids, into) {
                Some(kept) => edit.then(kept),
                None => edit,
            };
            if SuePaint::apply(cx, edit)
                && let Some(doc) = paint.doc_mut()
            {
                doc.layer = Some(into);
                doc.picked.clear();
            }
        }
        Err(error) => cx.fail(error),
    }
}

/// Change Palette Color: the offered color the command is named after
/// (`layer.palette_color.<name>`, `none` for No Palette Color), on every
/// picked layer.
fn palette_color(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    if let Some(name) = call.name.strip_prefix("layer.palette_color.") {
        paint.set_palette_color_named(cx, name);
    }
}

/// Convert Layer: its dialog, set to the kind the layer is not.
fn convert(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(info) = paint.layer_info(&cx.world.arena) {
        let vector = crate::vector::is_vector(&cx.world.arena, info.id);
        paint.dialog = Some(Dialog::Convert(ConvertDialog::new(
            info.id,
            u8::from(!vector),
            info.name,
        )));
    }
}

/// A command of this group, which works while a canvas is open unless
/// its `state` says more.
///
/// Every Layer command changes the picture or the view of it, so Auto
/// Actions may record each one (`RECORDABLE`); none of them leaves the
/// canvas (no `LEAVES_CANVAS`), so the test that runs every command
/// runs them all.
const fn c(
    name: &'static str,
    label: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "layer", run)
        .state(doc_open)
        .with(Flags::RECORDABLE)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("layer.new.raster_top", "new-raster-layer", new_raster_top).icon("layer-new"),
    c("layer.new", "raster-layer-2", new_layer)
        .icon("layer-new")
        .keys("Ctrl+Shift+N"),
    c("layer.new.vector", "vector-layer-2", new_vector).icon("layer-new-vector"),
    c("layer.new.text", "text", new_text).icon("tool-text"),
    c("layer.new.balloon", "balloon", new_balloon).icon("tool-balloon"),
    c("layer.new.gradient", "gradient-ellipsis", new_gradient).icon("tool-gradient"),
    c("layer.new.fill", "fill", new_fill).icon("tool-fill"),
    c("layer.new.tone", "tone-ellipsis", new_tone).icon("tone"),
    c("layer.new.paper", "paper-2", new_paper)
        .icon("layer-paper")
        .state(new_paper_state),
    c("layer.new.3d", "n-3d-layer", new_3d).icon("cube"),
    c("layer.new.figure", "n-3d-drawing-figure", new_figure).icon("material-pose"),
    c("layer.new.hand", "n-3d-hand", new_hand).icon("material-pose"),
    c("layer.new.head", "n-3d-head", new_head).icon("material-pose"),
    c("layer.new.frame", "frame-border-folder-ellipsis", new_frame).icon("tool-frame"),
    c(
        "layer.correction.brightness",
        "brightness-contrast-ellipsis",
        correction_brightness,
    )
    .icon("sun"),
    c(
        "layer.correction.levels",
        "level-correction-ellipsis",
        correction_levels,
    )
    .icon("palette-color-sliders"),
    c(
        "layer.correction.curve",
        "tone-curve-ellipsis",
        correction_curve,
    )
    .icon("pressure-curve"),
    c(
        "layer.correction.hue",
        "hue-saturation-luminosity-ellipsis",
        correction_hue,
    )
    .icon("palette-color-wheel"),
    c(
        "layer.correction.balance",
        "color-balance-ellipsis",
        correction_balance,
    )
    .icon("palette-color-sliders"),
    c(
        "layer.correction.invert",
        "reverse-gradient",
        correction_invert,
    )
    .icon("reverse"),
    c(
        "layer.correction.posterize",
        "posterization-ellipsis",
        correction_posterize,
    )
    .icon("palette-color-set"),
    c(
        "layer.correction.binarize",
        "binarization-ellipsis",
        correction_binarize,
    )
    .icon("layer-color-mode"),
    c(
        "layer.correction.gradient_map",
        "gradient-map-ellipsis",
        correction_gradient_map,
    )
    .icon("tool-gradient"),
    c(
        "layer.folder_insert",
        "create-folder-and-insert-layer",
        folder_insert,
    )
    .icon("move-to")
    .keys("Ctrl+G"),
    c("layer.new.folder", "new-layer-folder", new_folder).icon("layer-new-folder"),
    c("layer.ungroup", "ungroup-layer-folder", ungroup)
        .icon("folder-open")
        .keys("Ctrl+Shift+G"),
    c(
        "layer.from_selection",
        "new-layer-from-selection",
        from_selection,
    )
    .icon("copy")
    .state(from_selection_state),
    c("layer.duplicate", "duplicate-layer", duplicate).icon("layer-duplicate"),
    c("layer.delete", "delete-layer", delete).icon("delete"),
    c("layer.delete_empty", "empty-layers", delete_empty).icon("delete"),
    c("layer.delete_hidden", "hidden-layers", delete_hidden).icon("eye-off"),
    c(
        "layer.hide_drafts",
        "hide-all-visible-draft-layers",
        hide_drafts,
    )
    .icon("layer-draft"),
    c("layer.mask.outside", "mask-outside-selection", mask_outside)
        .icon("layer-mask-new")
        .state(new_mask_state),
    c("layer.mask.inside", "mask-selection", mask_inside)
        .icon("layer-mask-new")
        .state(new_mask_state),
    c("layer.mask.delete", "delete-mask", mask_delete)
        .icon("delete")
        .state(has_mask_state),
    c("layer.mask.apply", "apply-mask-to-layer", mask_apply)
        .icon("layer-mask-apply")
        .state(has_mask_state),
    c("layer.mask.toggle", "enable-mask", mask_toggle)
        .icon("layer-mask")
        .state(mask_on_state),
    c("layer.mask.edit", "edit-mask", mask_edit)
        .icon("edit")
        .state(mask_edit_state),
    c("layer.ruler.delete", "delete-ruler", delete_ruler)
        .icon("delete")
        .state(delete_ruler_state),
    c("layer.ruler.show", "show-ruler", show_rulers)
        .icon("tool-ruler")
        .state(show_rulers_state),
    c(
        "layer.ruler.perspective",
        "create-perspective-ruler-ellipsis",
        perspective_ruler,
    )
    .icon("tool-ruler"),
    c(
        "layer.ruler.symmetry",
        "create-symmetrical-ruler",
        symmetry_ruler,
    )
    .icon("tool-ruler"),
    c(
        "layer.ruler.circle",
        "create-concentric-circle-ruler",
        circle_ruler,
    )
    .icon("tool-ruler"),
    // Ruler/Frame's frame commands (FGV-1), on the panels the Object
    // tool picked.
    c(
        "layer.frame.divide_equally",
        "divide-frame-border-equally-ellipsis",
        divide_frame,
    )
    .icon("tool-frame")
    .state(frame_picked_state),
    c("layer.frame.combine", "combine-frames", combine_frames)
        .icon("tool-frame")
        .state(frame_picked_state),
    c("layer.toggle.visible", "show-layer", toggle_visible)
        .icon("eye")
        .state(visible_state),
    c("layer.rename", "change-layer-name-ellipsis", rename).icon("rename"),
    c("layer.toggle.clip", "clip-to-layer-below", toggle_clip)
        .icon("layer-clip")
        .keys("Ctrl+Alt+G")
        .state(clip_state),
    c(
        "layer.toggle.reference",
        "set-as-reference-layer",
        toggle_reference,
    )
    .icon("layer-reference")
    .state(reference_state),
    c("layer.toggle.draft", "set-as-draft-layer", toggle_draft)
        .icon("layer-draft")
        .state(draft_state),
    c("layer.toggle.lock", "lock-layer", toggle_lock)
        .icon("lock")
        .state(lock_state),
    c(
        "layer.toggle.lock_alpha",
        "lock-transparent-pixels",
        toggle_lock_alpha,
    )
    .icon("layer-lock-alpha")
    .state(on_raster),
    c(
        "layer.toggle.layer_color",
        "switch-using-layer-color",
        toggle_layer_color,
    )
    .icon("layer-color")
    .keys("Ctrl+B")
    .state(layer_color_state),
    c("layer.properties", "layer-property", properties).icon("layer-property"),
    c("layer.comps", "layer-comps-ellipsis", comps).icon("layers"),
    c("layer.select_above", "select-layer-above", select_above)
        .icon("chevron-up")
        .keys("Alt+]"),
    c("layer.select_below", "select-layer-below", select_below)
        .icon("chevron-down")
        .keys("Alt+["),
    c("layer.select_all", "select-all-layers", select_all).icon("all"),
    c("layer.select_none", "deselect-layers", select_none).icon("deselect"),
    c(
        "layer.selection.overlap",
        "select-overlapping-area",
        select_overlapping,
    )
    .icon("select-shrink")
    .state(overlap_state),
    c("layer.order.front", "bring-to-front", order_front).icon("sort-up"),
    c("layer.order.up", "move-up", order_up)
        .icon("arrow-up")
        .keys("Ctrl+]"),
    c("layer.order.down", "move-down", order_down)
        .icon("arrow-down")
        .keys("Ctrl+["),
    c("layer.order.back", "send-to-back", order_back).icon("sort-down"),
    c(
        "layer.transfer_down",
        "transfer-to-lower-layer",
        transfer_down,
    )
    .icon("layer-transfer-down"),
    c("layer.merge_down", "merge-with-layer-below", merge_down)
        .icon("layer-merge-down")
        .keys("Ctrl+E")
        .state(merge_down_state),
    c(
        "layer.merge_selected",
        "merge-selected-layers",
        merge_selected,
    )
    .icon("layer-merge-down")
    .keys("Alt+Shift+E")
    .state(merge_selected_state),
    c(
        "layer.merge_visible_here",
        "merge-visible-layers",
        merge_visible_here,
    )
    .icon("layers")
    .keys("Ctrl+Shift+E"),
    c(
        "layer.merge_visible",
        "merge-visible-to-new-layer",
        merge_visible,
    )
    .icon("layers"),
    c("layer.flatten", "flatten-image", flatten).icon("picture"),
    c("layer.rasterize", "rasterize", rasterize)
        .icon("picture")
        .state(rasterize_state),
    c("layer.convert", "convert-layer-ellipsis", convert)
        .icon("replace")
        .state(on_raster),
    c(
        "layer.lines_tones_vector",
        "convert-to-vector-lines-and-tones",
        lines_tones_vector,
    )
    .icon("extract-line"),
    c(
        "layer.update_lines",
        "update-lines-to-new-brush-engine",
        crate::canvas::lines::update_lines,
    )
    .icon("layer-new-vector")
    .state(crate::canvas::lines::on_vector_layer),
    c(
        "layer.palette_color.none",
        "no-palette-color",
        palette_color,
    )
    .icon("palette-color-set"),
    c(
        "layer.palette_color.red",
        "palette-color-red",
        palette_color,
    )
    .icon("palette-color-set"),
    c(
        "layer.palette_color.orange",
        "palette-color-orange",
        palette_color,
    )
    .icon("palette-color-set"),
    c(
        "layer.palette_color.yellow",
        "palette-color-yellow",
        palette_color,
    )
    .icon("palette-color-set"),
    c(
        "layer.palette_color.green",
        "palette-color-green",
        palette_color,
    )
    .icon("palette-color-set"),
    c(
        "layer.palette_color.blue",
        "palette-color-blue",
        palette_color,
    )
    .icon("palette-color-set"),
    c(
        "layer.palette_color.purple",
        "palette-color-purple",
        palette_color,
    )
    .icon("palette-color-set"),
];

#[cfg(test)]
mod tests {
    use super::{COMMANDS, on_canvas};
    use crate::select::{Op, Selection};
    use sue_wm::Flags;

    #[test]
    fn select_overlapping_area_keeps_what_the_layer_covers() {
        // A layer of 2 by 2 pixels, one empty and one half drawn, lying
        // one pixel in from the top left of a 3 by 3 canvas.
        let alpha = Selection::from_mask(2, 2, vec![255, 0, 128, 255]).expect("a mask");
        let drawn = on_canvas(&alpha, [1, 1], [3, 3]);
        assert_eq!(drawn, [0, 0, 0, 0, 255, 0, 0, 128, 255]);
        // Pushed off the canvas, only what still lies on it counts.
        assert_eq!(
            on_canvas(&alpha, [-1, 1], [3, 3]),
            [0, 0, 0, 0, 0, 0, 255, 0, 0]
        );
        // The selection (its middle column at half) keeps the overlap,
        // as much as both cover.
        let mut selection =
            Selection::from_mask(3, 3, vec![255, 128, 255, 255, 128, 255, 255, 128, 255])
                .expect("a selection");
        selection.apply_mask(&drawn, Op::Intersect);
        assert_eq!(selection.mask(), [0, 0, 0, 0, 128, 0, 0, 128, 255]);
    }

    #[test]
    fn every_layer_command_has_an_icon_of_the_shared_set() {
        for command in COMMANDS {
            assert!(
                sue_wm::icons::Icon::from_name(command.icon).is_some(),
                "{}: no icon {:?}",
                command.name,
                command.icon
            );
        }
    }

    #[test]
    fn the_flags_say_what_auto_actions_and_the_canvas_test_did() {
        // Auto Actions record every Layer command, as `recordable()`
        // let every `layer.` name through, and the test that runs every
        // command skips nothing of this group. The names are checked
        // here, not through `recordable()`, which P3-14 deletes.
        for command in COMMANDS {
            assert_eq!(command.group, "layer", "{}", command.name);
            assert!(command.name.starts_with("layer."), "{}", command.name);
            assert!(command.flags.has(Flags::RECORDABLE), "{}", command.name);
            assert!(!command.flags.has(Flags::LEAVES_CANVAS), "{}", command.name);
            assert!(!command.is_family(), "{}", command.name);
        }
    }
}
