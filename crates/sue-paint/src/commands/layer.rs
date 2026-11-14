//! The Layer menu's commands: one entry each, with what it does and how
//! its item looks.

use sue_core::txn::SetProperty;
use sue_core::{Arena, EntityId, PixelFormat, RasterTile2D, ReversibleTransaction};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, Call, Command, Facts, ItemState};

use super::{doc_open, flag, when};
use crate::dialogs::Dialog;
use crate::doc;
use crate::filters::Filter;
use crate::studio::SuePaint;

/// Runs a Layer command by its name: Window, Layer Property runs Layer
/// Property through it, since that item is the Layer menu's.
pub(super) fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    if call.name == "window.layer_property" {
        properties(paint, cx, call);
    } else if let Some(command) = COMMANDS.iter().find(|command| command.name == call.name) {
        (command.run)(paint, cx, call);
    }
}

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

fn merge_selected_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.doc().is_some_and(|doc| !doc.picked.is_empty()) && raster(paint, facts))
}

fn from_selection_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(selection(paint) && raster(paint, facts))
}

fn merge_down_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    let arena = facts.arena;
    let below_raster = paint.layer_info(arena).is_some_and(|info| {
        let stack = doc::layers(arena, paint.picture().unwrap_or_default());
        stack
            .iter()
            .position(|layer| layer.id == info.id)
            .and_then(|at| at.checked_sub(1))
            .is_some_and(|below| stack[below].kind == LayerKind::Raster)
    });
    when(raster(paint, facts) && below_raster)
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

fn reference_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint
        .layer_info(facts.arena)
        .map_or(ItemState::off(), |info| {
            flag(doc::is_reference(facts.arena, info.id))
        })
}

fn rasterize_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint
            .layer_info(facts.arena)
            .is_some_and(|info| crate::vector::is_vector(facts.arena, info.id)),
    )
}

/// Mask Outside Selection works on a raster layer without a mask; Mask
/// Selection needs a selection too. Either is checked while there is a
/// mask.
fn new_mask_state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    let masked = masked(paint, facts);
    let mut state = when(
        raster(paint, facts) && !masked && (call.name == "layer.mask.outside" || selection(paint)),
    );
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

/// A new layer of tone, in the selection if there is one.
fn new_tone(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.run("layer.new", cx);
    paint.run("filter.tone", cx);
}

fn new_frame(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Frame(crate::comic::FrameSpec::default()));
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
        paint.dialog = Some(Dialog::Perspective(2));
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

/// Deletes the current layer, or the current folder with what it holds;
/// the last layer stays.
fn delete(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let (Some(picture), Some(layer)) = (paint.picture(), paint.layer()) {
        if doc::is_folder(&cx.world.arena, layer) {
            let edit = crate::anim::delete_folder(&cx.world.arena, picture, layer);
            SuePaint::apply(cx, edit);
            return;
        }
        if doc::layers(&cx.world.arena, picture).len() <= 1 {
            cx.fail(tr!("picture-keeps-least-one-layer"));
            return;
        }
        SuePaint::apply(cx, doc::delete(layer));
    }
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

/// The Layer palette, on its Layer Property tab.
fn properties(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.layer = true;
    paint.layer_tab = 1;
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
    let (edit, folder) = doc::new_folder(&cx.world.arena, picture, layer.or(paint.layer()), insert);
    apply_and_pick(paint, cx, edit, folder);
}

fn folder_insert(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    folder(paint, cx, true);
}

fn new_folder(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    folder(paint, cx, false);
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

/// A layer the size of the canvas with the drawing color at the top
/// fading into the second color at the bottom.
fn new_gradient(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let arena = &cx.world.arena;
    let Some([width, height]) = arena.get_by_id(picture).ok().and_then(picture::size) else {
        return;
    };
    let (from, to) = (paint.main, paint.sub);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let t = if height > 1 {
            y as f32 / (height - 1) as f32
        } else {
            0.0
        };
        let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        let row = [
            mix(from[0], to[0]),
            mix(from[1], to[1]),
            mix(from[2], to[2]),
            255,
        ];
        for _ in 0..width {
            pixels.extend_from_slice(&row);
        }
    }
    let Ok(raster) = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels) else {
        return;
    };
    match doc::add_layer_pixels(arena, picture, paint.layer(), tr!("gradient"), raster) {
        Ok((spawn, id)) => {
            let edit = doc::Edit::new(tr!("new-gradient-layer")).then(spawn);
            apply_and_pick(paint, cx, edit, id);
        }
        Err(error) => cx.fail(error),
    }
}

/// The picked layers and the current one merged into one.
fn merge_selected(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let (Some(picture), Some(doc)) = (paint.picture(), paint.doc()) else {
        return;
    };
    let mut ids = doc.picked.clone();
    ids.extend(doc.layer);
    match doc::merge_layers(&cx.world.arena, picture, &ids) {
        Ok((edit, into)) => {
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

/// Convert Layer: its dialog, set to the kind the layer is not.
fn convert(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(info) = paint.layer_info(&cx.world.arena) {
        let vector = crate::vector::is_vector(&cx.world.arena, info.id);
        paint.dialog = Some(Dialog::Convert(info.id, u8::from(!vector), info.name));
    }
}

/// A command of this group, which works while a canvas is open unless
/// its `state` says more.
const fn c(
    name: &'static str,
    label: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "layer", run).state(doc_open)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("layer.new.raster_top", "new-raster-layer", new_raster_top),
    c("layer.new", "raster-layer-2", new_layer).keys("Ctrl+Shift+N"),
    c("layer.new.vector", "vector-layer-2", new_vector),
    c("layer.new.text", "text", new_text),
    c("layer.new.balloon", "balloon", new_balloon),
    c("layer.new.gradient", "gradient-ellipsis", new_gradient),
    c("layer.new.fill", "fill", new_fill),
    c("layer.new.tone", "tone-ellipsis", new_tone),
    c("layer.new.paper", "paper-2", new_paper).state(new_paper_state),
    c("layer.new.3d", "n-3d-layer", new_3d),
    c("layer.new.figure", "n-3d-drawing-figure", new_figure),
    c("layer.new.hand", "n-3d-hand", new_hand),
    c("layer.new.head", "n-3d-head", new_head),
    c("layer.new.frame", "frame-border-folder-ellipsis", new_frame),
    c(
        "layer.correction.brightness",
        "brightness-contrast-ellipsis",
        correction_brightness,
    ),
    c(
        "layer.correction.levels",
        "level-correction-ellipsis",
        correction_levels,
    ),
    c(
        "layer.correction.curve",
        "tone-curve-ellipsis",
        correction_curve,
    ),
    c(
        "layer.correction.hue",
        "hue-saturation-luminosity-ellipsis",
        correction_hue,
    ),
    c(
        "layer.correction.balance",
        "color-balance-ellipsis",
        correction_balance,
    ),
    c(
        "layer.correction.invert",
        "reverse-gradient",
        correction_invert,
    ),
    c(
        "layer.correction.posterize",
        "posterization-ellipsis",
        correction_posterize,
    ),
    c(
        "layer.correction.binarize",
        "binarization-ellipsis",
        correction_binarize,
    ),
    c(
        "layer.correction.gradient_map",
        "gradient-map-ellipsis",
        correction_gradient_map,
    ),
    c(
        "layer.folder_insert",
        "create-folder-and-insert-layer",
        folder_insert,
    )
    .keys("Ctrl+G"),
    c("layer.new.folder", "new-layer-folder", new_folder),
    c("layer.ungroup", "ungroup-layer-folder", ungroup).keys("Ctrl+Shift+G"),
    c(
        "layer.from_selection",
        "new-layer-from-selection",
        from_selection,
    )
    .state(from_selection_state),
    c("layer.duplicate", "duplicate-layer", duplicate),
    c("layer.delete", "delete-layer", delete),
    c("layer.delete_empty", "empty-layers", delete_empty),
    c("layer.delete_hidden", "hidden-layers", delete_hidden),
    c(
        "layer.hide_drafts",
        "hide-all-visible-draft-layers",
        hide_drafts,
    ),
    c("layer.mask.outside", "mask-outside-selection", mask_outside).state(new_mask_state),
    c("layer.mask.inside", "mask-selection", mask_inside).state(new_mask_state),
    c("layer.mask.delete", "delete-mask", mask_delete).state(has_mask_state),
    c("layer.mask.apply", "apply-mask-to-layer", mask_apply).state(has_mask_state),
    c("layer.mask.toggle", "enable-mask", mask_toggle).state(mask_on_state),
    c("layer.mask.edit", "edit-mask", mask_edit).state(mask_edit_state),
    c("layer.ruler.delete", "delete-ruler", delete_ruler).state(delete_ruler_state),
    c("layer.ruler.show", "show-ruler", show_rulers).state(show_rulers_state),
    c(
        "layer.ruler.perspective",
        "create-perspective-ruler-ellipsis",
        perspective_ruler,
    ),
    c(
        "layer.ruler.symmetry",
        "create-symmetrical-ruler",
        symmetry_ruler,
    ),
    c(
        "layer.ruler.circle",
        "create-concentric-circle-ruler",
        circle_ruler,
    ),
    c("layer.toggle.visible", "show-layer", toggle_visible).state(visible_state),
    c("layer.rename", "change-layer-name-ellipsis", rename),
    c("layer.toggle.clip", "clip-to-layer-below", toggle_clip)
        .keys("Ctrl+Alt+G")
        .state(clip_state),
    c(
        "layer.toggle.reference",
        "set-as-reference-layer",
        toggle_reference,
    )
    .state(reference_state),
    c("layer.toggle.draft", "set-as-draft-layer", toggle_draft).state(draft_state),
    c("layer.toggle.lock", "lock-layer", toggle_lock).state(lock_state),
    c(
        "layer.toggle.lock_alpha",
        "lock-transparent-pixels",
        toggle_lock_alpha,
    )
    .state(on_raster),
    c("layer.properties", "layer-property", properties),
    c("layer.comps", "layer-comps-ellipsis", comps),
    c("layer.select_above", "select-layer-above", select_above).keys("Alt+]"),
    c("layer.select_below", "select-layer-below", select_below).keys("Alt+["),
    c("layer.select_all", "select-all-layers", select_all),
    c("layer.select_none", "deselect-layers", select_none),
    c("layer.order.front", "bring-to-front", order_front),
    c("layer.order.up", "move-up", order_up).keys("Ctrl+]"),
    c("layer.order.down", "move-down", order_down).keys("Ctrl+["),
    c("layer.order.back", "send-to-back", order_back),
    c(
        "layer.transfer_down",
        "transfer-to-lower-layer",
        transfer_down,
    ),
    c("layer.merge_down", "merge-with-layer-below", merge_down)
        .keys("Ctrl+E")
        .state(merge_down_state),
    c(
        "layer.merge_selected",
        "merge-selected-layers",
        merge_selected,
    )
    .keys("Alt+Shift+E")
    .state(merge_selected_state),
    c(
        "layer.merge_visible_here",
        "merge-visible-layers",
        merge_visible_here,
    )
    .keys("Ctrl+Shift+E"),
    c(
        "layer.merge_visible",
        "merge-visible-to-new-layer",
        merge_visible,
    ),
    c("layer.flatten", "flatten-image", flatten),
    c("layer.rasterize", "rasterize", rasterize).state(rasterize_state),
    c("layer.convert", "convert-layer-ellipsis", convert).state(on_raster),
    c(
        "layer.lines_tones_vector",
        "convert-to-vector-lines-and-tones",
        lines_tones_vector,
    ),
];
