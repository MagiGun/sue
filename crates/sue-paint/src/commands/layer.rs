//! The Layer menu's commands.

use sue_core::txn::SetProperty;
use sue_core::{Arena, EntityId, PixelFormat, RasterTile2D, UndoStack};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, Call, Command, Facts, ItemState, World};

use super::{Basis, flag, when};
use crate::dialogs::Dialog;
use crate::doc;
use crate::filters::Filter;
use crate::studio::SuePaint;

/// Does what a Layer command does.
pub(super) fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.run_layer(call.name, cx);
}

/// How an item for a Layer command looks right now.
pub(super) fn state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint.state_layer(call.name, facts.arena, facts.history)
}

impl SuePaint {
    /// How an item for a Layer command looks right now.
    fn state_layer(&self, command: &str, arena: &Arena, _history: &UndoStack<World>) -> ItemState {
        let Basis {
            has_doc,
            info,
            raster,
            selection,
        } = self.basis(arena);
        match command {
            "layer.merge_selected" => {
                when(self.doc().is_some_and(|doc| !doc.picked.is_empty()) && raster)
            }
            "layer.convert" => when(raster),
            "layer.from_selection" => when(selection && raster),
            "layer.toggle.lock_alpha" => when(raster),
            "layer.merge_down" => {
                let below_raster = info.as_ref().is_some_and(|info| {
                    let stack = doc::layers(arena, self.picture().unwrap_or_default());
                    stack
                        .iter()
                        .position(|layer| layer.id == info.id)
                        .and_then(|at| at.checked_sub(1))
                        .is_some_and(|below| stack[below].kind == LayerKind::Raster)
                });
                when(raster && below_raster)
            }
            "layer.new.paper" => when(
                has_doc
                    && !doc::layers(arena, self.picture().unwrap_or_default())
                        .iter()
                        .any(|layer| layer.kind == LayerKind::Paper),
            ),
            "layer.toggle.visible" => {
                info.map_or(ItemState::off(), |info| flag(info.style.visible))
            }
            "layer.toggle.clip" => info.map_or(ItemState::off(), |info| flag(info.style.clip)),
            "layer.mask.outside" | "layer.mask.inside" => {
                let masked = info
                    .as_ref()
                    .is_some_and(|info| doc::mask_of(arena, info.id).is_some());
                let mut state =
                    when(raster && !masked && (command == "layer.mask.outside" || selection));
                state.checked = Some(masked);
                state
            }
            "layer.mask.delete" | "layer.mask.apply" => when(
                info.as_ref()
                    .is_some_and(|info| doc::mask_of(arena, info.id).is_some()),
            ),
            "layer.mask.toggle" | "layer.mask.edit" => match info
                .as_ref()
                .filter(|info| doc::mask_of(arena, info.id).is_some())
            {
                Some(info) => flag(if command == "layer.mask.toggle" {
                    doc::mask_on(arena, info.id)
                } else {
                    self.doc().is_some_and(|doc| doc.mask_edit)
                }),
                None => ItemState::off(),
            },
            "layer.toggle.draft" => info.map_or(ItemState::off(), |info| flag(info.style.draft)),
            "layer.rasterize" => when(
                info.as_ref()
                    .is_some_and(|info| crate::vector::is_vector(arena, info.id)),
            ),
            "layer.toggle.reference" => info.map_or(ItemState::off(), |info| {
                flag(doc::is_reference(arena, info.id))
            }),
            "layer.toggle.lock" => info.map_or(ItemState::off(), |info| flag(info.locked)),
            "layer.ruler.show" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.show_rulers)),
            "layer.ruler.delete" => when(
                self.doc()
                    .is_some_and(|doc| !crate::rulers::Rulers::of(arena, doc.picture).is_empty()),
            ),
            _ => when(has_doc),
        }
    }

    /// Does what a Layer command does.
    #[allow(clippy::too_many_lines, reason = "one arm per command")]
    fn run_layer(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            "layer.from_selection" => {
                let kept = self.clipboard.take();
                if self.copy(cx) {
                    self.paste(cx, &tr!("from-selection"));
                }
                self.clipboard = kept;
            }
            // Clip Studio Paint's Convert to lines and tones with the lines
            // as a vector layer: a 3D layer's outline and folds as strokes.
            "layer.lines_tones_vector" => {
                match self
                    .layer()
                    .filter(|layer| crate::three::is_3d(&cx.world.arena, *layer))
                {
                    Some(layer) => self.lines_and_tones_3d(cx, layer, true),
                    None => cx.notify(tr!("pick-a-3d-layer-first")),
                }
            }
            "layer.new.raster_top" => {
                self.run("layer.new", cx);
            }
            "layer.new.tone" => {
                // A new layer of tone, in the selection if there is one.
                self.run("layer.new", cx);
                self.run("filter.tone", cx);
            }
            "layer.new.frame" => {
                self.dialog = Some(Dialog::Frame(crate::comic::FrameSpec::default()));
            }
            "layer.new.balloon" => self.add_balloon_layer(cx),
            command if command.starts_with("layer.correction.") => {
                let filter = match command.trim_start_matches("layer.correction.") {
                    "brightness" => Filter::BrightnessContrast {
                        brightness: 0.0,
                        contrast: 0.0,
                    },
                    "levels" => Filter::Levels {
                        black: 0.0,
                        white: 1.0,
                        gamma: 1.0,
                        out_black: 0.0,
                        out_white: 1.0,
                    },
                    "curve" => Filter::ToneCurve {
                        rgb: sue_model::Response::LINEAR,
                        red: sue_model::Response::LINEAR,
                        green: sue_model::Response::LINEAR,
                        blue: sue_model::Response::LINEAR,
                        channel: 0,
                    },
                    "hue" => Filter::HueSaturation {
                        hue: 0.0,
                        saturation: 0.0,
                        luminosity: 0.0,
                    },
                    "balance" => Filter::ColorBalance {
                        shifts: [[0.0; 3]; 3],
                        range: 1,
                    },
                    "invert" => Filter::Invert,
                    "posterize" => Filter::Posterize { levels: 4 },
                    "binarize" => Filter::Binarize { threshold: 0.5 },
                    "gradient_map" => Filter::GradientMap {
                        from: self.main,
                        to: self.sub,
                    },
                    _ => {
                        cx.notify(tr!("correction-not-sue-paint-yet"));
                        return;
                    }
                };
                let Some(picture) = self.picture() else {
                    return;
                };
                let text = filter.to_text().unwrap_or_default();
                let (edit, layer) = doc::new_correction(
                    &cx.world.arena,
                    picture,
                    self.layer(),
                    &filter.label(),
                    &text,
                );
                if Self::apply(cx, edit) {
                    if let Some(doc) = self.doc_mut() {
                        doc.layer = Some(layer);
                    }
                    if filter != Filter::Invert {
                        self.dialog = Some(Dialog::Correction(layer, filter, text, true));
                    }
                }
            }
            "layer.new.text" => {
                let middle = self
                    .picture()
                    .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                    .and_then(picture::size)
                    .map_or([0.0, 0.0], |[w, h]| [w as f32 / 2.0, h as f32 / 2.0]);
                let mut lettering = crate::lettering::Lettering::new(middle, self.drawing_color());
                lettering.center = true;
                self.dialog = Some(Dialog::Text(None, lettering));
            }
            "layer.ruler.show" => {
                if let Some(doc) = self.doc_mut() {
                    doc.show_rulers = !doc.show_rulers;
                }
            }
            "layer.ruler.perspective" => {
                if self.picture().is_some() {
                    self.dialog = Some(Dialog::Perspective(2));
                }
            }
            "layer.ruler.symmetry" | "layer.ruler.circle" => {
                let Some(picture) = self.picture() else {
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
                let (key, text, label, mode) = if command == "layer.ruler.symmetry" {
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
                if Self::apply(cx, crate::rulers::edit(picture, key, text, &label)) {
                    self.pick_ruler(mode);
                    if mode == 2 {
                        self.toolbox.current_mut().kind = 2;
                    }
                    if let Some(doc) = self.doc_mut() {
                        doc.ruler_on = true;
                        doc.show_rulers = true;
                    }
                }
            }
            "layer.ruler.delete" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let rulers = crate::rulers::Rulers::of(&cx.world.arena, picture);
                let mut edit = doc::Edit::new(tr!("delete-ruler"));
                for key in crate::rulers::ALL {
                    if rulers.text(key).is_some() {
                        edit = edit.then(SetProperty::remove(picture, key));
                    }
                }
                self.vanishing_line = None;
                if !edit.is_empty() {
                    Self::apply(cx, edit);
                }
            }
            "layer.new" | "layer.new.fill" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let arena = &cx.world.arena;
                let name = if command == "layer.new.fill" {
                    tr!("fill")
                } else {
                    doc::new_layer_name(arena, picture)
                };
                let fill = (command == "layer.new.fill").then(|| {
                    let rgb = self.drawing_color();
                    [rgb[0], rgb[1], rgb[2], 255]
                });
                match doc::add_raster_layer(arena, picture, self.layer(), name, fill) {
                    Ok((spawn, id)) => {
                        if Self::apply(cx, spawn)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.new.vector" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let name = doc::new_layer_name(&cx.world.arena, picture);
                match doc::add_vector_layer(&cx.world.arena, picture, self.layer(), name) {
                    Ok((spawn, id)) => {
                        if Self::apply(cx, spawn)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.new.paper" => {
                if let Some(picture) = self.picture() {
                    let (spawn, _) = doc::add_paper(
                        &cx.world.arena,
                        picture,
                        sue_core::Color::srgb8(255, 255, 255),
                    );
                    Self::apply(cx, spawn);
                }
            }
            "layer.duplicate" => {
                if let (Some(picture), Some(layer)) = (self.picture(), self.layer()) {
                    match doc::duplicate(&cx.world.arena, picture, layer) {
                        Ok((spawn, id)) => {
                            if Self::apply(cx, spawn)
                                && let Some(doc) = self.doc_mut()
                            {
                                doc.layer = Some(id);
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            "layer.delete" => {
                if let (Some(picture), Some(layer)) = (self.picture(), self.layer()) {
                    if doc::is_folder(&cx.world.arena, layer) {
                        let edit = crate::anim::delete_folder(&cx.world.arena, picture, layer);
                        Self::apply(cx, edit);
                        return;
                    }
                    if doc::layers(&cx.world.arena, picture).len() <= 1 {
                        cx.fail(tr!("picture-keeps-least-one-layer"));
                        return;
                    }
                    Self::apply(cx, doc::delete(layer));
                }
            }
            "layer.toggle.visible" => self.toggle_layer(cx, picture::VISIBLE, true),
            "layer.comps" => {
                if self.picture().is_some() {
                    self.dialog = Some(Dialog::Comps(crate::dialogs::CompsDialog::default()));
                }
            }
            "layer.new.3d" => {
                if self.picture().is_some() {
                    self.dialog = Some(Dialog::Shape3D(0));
                }
            }
            // Clip Studio Paint's 3D drawing figure, standing, to pose.
            "layer.new.figure" => {
                let name = tr!("drawing-figure");
                self.add_3d_layer(cx, &name, crate::figure::new_model(&name));
            }
            // Clip Studio Paint 5.0's 3D hand and 5.1's 3D head, to pose.
            "layer.new.hand" => {
                let name = tr!("n-3d-hand");
                self.add_3d_layer(
                    cx,
                    &name,
                    crate::figure::new_of(&name, crate::figure::Kind::Hand),
                );
            }
            "layer.new.head" => {
                let name = tr!("n-3d-head");
                self.add_3d_layer(
                    cx,
                    &name,
                    crate::figure::new_of(&name, crate::figure::Kind::Head),
                );
            }
            "layer.rasterize" => {
                let (Some(picture), Some(layer)) = (self.picture(), self.layer()) else {
                    return;
                };
                match doc::rasterize(&cx.world.arena, picture, layer) {
                    Ok((edit, id)) => {
                        if Self::apply(cx, edit)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.toggle.reference" => {
                if let Some(layer) = self.layer() {
                    let on = doc::is_reference(&cx.world.arena, layer);
                    Self::apply(cx, doc::set(layer, doc::REFERENCE, !on));
                }
            }
            "layer.toggle.clip" => self.toggle_layer(cx, picture::CLIP, false),
            "layer.mask.outside" | "layer.mask.inside" => {
                let Some(layer) = self.layer() else {
                    return;
                };
                let selection = self.doc().and_then(|doc| doc.selection.clone());
                if command == "layer.mask.inside" && selection.is_none() {
                    cx.fail(tr!("select-what-to-hide-first"));
                    return;
                }
                let inside = command == "layer.mask.inside";
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
                        Self::apply(cx, edit);
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.mask.delete" => {
                if let Some(edit) = self
                    .layer()
                    .and_then(|layer| doc::delete_mask(&cx.world.arena, layer))
                {
                    Self::apply(cx, edit);
                    if let Some(doc) = self.doc_mut() {
                        doc.mask_edit = false;
                    }
                }
            }
            "layer.mask.apply" => {
                if let Some(layer) = self.layer() {
                    match doc::apply_mask(&cx.world.arena, layer) {
                        Ok(edit) => {
                            Self::apply(cx, edit);
                            if let Some(doc) = self.doc_mut() {
                                doc.mask_edit = false;
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            "layer.mask.toggle" => {
                if let Some(layer) = self.layer() {
                    let on = doc::mask_on(&cx.world.arena, layer);
                    Self::apply(cx, doc::set(layer, doc::MASK_ON, !on));
                }
            }
            "layer.mask.edit" => {
                let masked = self
                    .layer()
                    .is_some_and(|layer| doc::mask_of(&cx.world.arena, layer).is_some());
                if let Some(doc) = self.doc_mut() {
                    doc.mask_edit = masked && !doc.mask_edit;
                }
            }
            "layer.toggle.draft" => self.toggle_layer(cx, picture::DRAFT, false),
            "layer.toggle.lock" => self.toggle_layer(cx, picture::LOCKED, false),
            "layer.toggle.lock_alpha" => self.toggle_layer(cx, picture::ALPHA_LOCKED, false),
            "layer.properties" | "window.layer_property" => {
                self.panels.layer = true;
                self.layer_tab = 1;
            }
            // Clip Studio Paint's Select all layers: every layer picked
            // with the current one (for merging or correcting them all).
            "layer.select_all" | "layer.select_none" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let current = self.layer();
                let stack = if command == "layer.select_all" {
                    doc::layers(&cx.world.arena, picture)
                } else {
                    Vec::new()
                };
                let all: Vec<EntityId> = stack
                    .into_iter()
                    .map(|info| info.id)
                    .filter(|id| Some(*id) != current)
                    .collect();
                if let Some(doc) = self.doc_mut() {
                    doc.picked = all;
                }
            }
            "layer.select_above" | "layer.select_below" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let stack = doc::layers(&cx.world.arena, picture);
                let at = self
                    .layer()
                    .and_then(|layer| stack.iter().position(|info| info.id == layer));
                let next = match (at, command == "layer.select_above") {
                    (Some(at), true) => stack.get(at + 1),
                    (Some(at), false) => at.checked_sub(1).and_then(|at| stack.get(at)),
                    (None, _) => stack.last(),
                };
                if let (Some(next), Some(doc)) = (next.map(|info| info.id), self.doc_mut()) {
                    doc.layer = Some(next);
                }
            }
            "layer.folder_insert" | "layer.new.folder" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let layer = self
                    .layer()
                    .filter(|layer| !doc::is_folder(&cx.world.arena, *layer));
                let (edit, folder) = doc::new_folder(
                    &cx.world.arena,
                    picture,
                    layer.or(self.layer()),
                    command == "layer.folder_insert",
                );
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(folder);
                }
            }
            "layer.order.up" | "layer.order.down"
                if self.move_in_folders(cx, command == "layer.order.up") => {}
            "layer.order.front" | "layer.order.up" | "layer.order.down" | "layer.order.back" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let stack = doc::layers(&cx.world.arena, picture);
                let Some(at) = self
                    .layer()
                    .and_then(|layer| stack.iter().position(|info| info.id == layer))
                else {
                    return;
                };
                let last = stack.len().saturating_sub(1);
                let to = match command {
                    "layer.order.front" => last,
                    "layer.order.up" => (at + 1).min(last),
                    "layer.order.down" => at.saturating_sub(1),
                    _ => 0,
                };
                self.move_layer_to(cx, to);
            }
            "layer.merge_down" => {
                if let (Some(picture), Some(layer)) = (self.picture(), self.layer()) {
                    match doc::merge_down(&cx.world.arena, picture, layer) {
                        Ok((edit, into)) => {
                            if Self::apply(cx, edit)
                                && let Some(doc) = self.doc_mut()
                            {
                                doc.layer = Some(into);
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            "layer.ungroup" => {
                let (Some(picture), Some(folder)) = (self.picture(), self.layer()) else {
                    return;
                };
                let arena = &cx.world.arena;
                if !doc::is_folder(arena, folder) {
                    cx.fail(tr!("pick-a-layer-folder-first"));
                    return;
                }
                // Its layers go where it was; then it goes.
                let outer = doc::folder_of(arena, folder);
                let mut edit = doc::Edit::new(tr!("ungroup-layer-folder"));
                for layer in doc::layers(arena, picture) {
                    if doc::folder_of(arena, layer.id) == Some(folder)
                        && let Some(moved) = doc::set_folder(arena, layer.id, outer)
                    {
                        edit = edit.then(moved);
                    }
                }
                // An animation folder's track goes with it.
                if let Some(sequence) = crate::anim::timeline(arena, picture)
                    && let Some(track) = crate::anim::track_of(sequence, folder)
                {
                    edit = edit.then(sue_core::txn::SetExtension::set(
                        picture,
                        crate::anim::without_track(sequence, track),
                    ));
                }
                edit = edit.then(sue_core::txn::DespawnEntity::new(folder));
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = None;
                }
            }
            "layer.delete_empty" | "layer.delete_hidden" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let arena = &cx.world.arena;
                let stack = doc::layers(arena, picture);
                let gone: Vec<EntityId> = stack
                    .iter()
                    .filter(|info| {
                        info.kind == LayerKind::Raster && !doc::is_folder(arena, info.id)
                    })
                    .filter(|info| {
                        if command == "layer.delete_hidden" {
                            !info.style.visible
                        } else {
                            doc::raster(arena, info.id).is_some_and(|raster| {
                                raster.bytes().is_some_and(|bytes| {
                                    bytes.iter().skip(3).step_by(4).all(|a| *a == 0)
                                })
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
                let mut edit = doc::Edit::new(if command == "layer.delete_hidden" {
                    tr!("hidden-layers")
                } else {
                    tr!("empty-layers")
                });
                for id in gone {
                    edit = edit.then(sue_core::txn::DespawnEntity::new(id));
                }
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = None;
                }
            }
            "layer.hide_drafts" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let mut edit = doc::Edit::new(tr!("hide-all-visible-draft-layers"));
                for info in doc::layers(&cx.world.arena, picture) {
                    if info.style.draft && info.style.visible {
                        edit = edit.then(SetProperty::new(info.id, picture::VISIBLE, false));
                    }
                }
                if !edit.is_empty() {
                    Self::apply(cx, edit);
                }
            }
            "layer.rename" => {
                if let Some(info) = self.layer_info(&cx.world.arena) {
                    self.panels.layer = true;
                    self.layer_tab = 0;
                    self.renaming = Some((info.id, info.name));
                }
            }
            "layer.merge_visible_here" => {
                // Merge Visible Layers: the visible layers become one, the
                // hidden ones stay.
                let Some(picture) = self.picture() else {
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
                        if Self::apply(cx, edit)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.transfer_down" => {
                if let (Some(picture), Some(layer)) = (self.picture(), self.layer()) {
                    match doc::transfer_down(&cx.world.arena, picture, layer) {
                        Ok((edit, _)) => {
                            Self::apply(cx, edit);
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            "layer.merge_visible" | "layer.flatten" => {
                if let Some(picture) = self.picture() {
                    match doc::merge_visible(&cx.world.arena, picture, command == "layer.flatten") {
                        Ok((edit, id)) => {
                            if Self::apply(cx, edit)
                                && let Some(doc) = self.doc_mut()
                            {
                                doc.layer = Some(id);
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            "layer.new.gradient" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let arena = &cx.world.arena;
                let Some([width, height]) = arena.get_by_id(picture).ok().and_then(picture::size)
                else {
                    return;
                };
                let (from, to) = (self.main, self.sub);
                let mut pixels = Vec::with_capacity((width * height * 4) as usize);
                for y in 0..height {
                    let t = if height > 1 {
                        y as f32 / (height - 1) as f32
                    } else {
                        0.0
                    };
                    let mix = |a: u8, b: u8| {
                        (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8
                    };
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
                let Ok(raster) =
                    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
                else {
                    return;
                };
                match doc::add_layer_pixels(arena, picture, self.layer(), tr!("gradient"), raster) {
                    Ok((spawn, id)) => {
                        if Self::apply(cx, doc::Edit::new(tr!("new-gradient-layer")).then(spawn))
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.merge_selected" => {
                let (Some(picture), Some(doc)) = (self.picture(), self.doc()) else {
                    return;
                };
                let mut ids = doc.picked.clone();
                ids.extend(doc.layer);
                match doc::merge_layers(&cx.world.arena, picture, &ids) {
                    Ok((edit, into)) => {
                        if Self::apply(cx, edit)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(into);
                            doc.picked.clear();
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "layer.convert" => {
                if let Some(info) = self.layer_info(&cx.world.arena) {
                    let vector = crate::vector::is_vector(&cx.world.arena, info.id);
                    self.dialog = Some(Dialog::Convert(info.id, u8::from(!vector), info.name));
                }
            }
            _ => {}
        }
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "layer", run).state(state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("layer.new.raster_top", "new-raster-layer"),
    c("layer.new", "raster-layer-2").keys("Ctrl+Shift+N"),
    c("layer.new.vector", "vector-layer-2"),
    c("layer.new.text", "text"),
    c("layer.new.balloon", "balloon"),
    c("layer.new.gradient", "gradient-ellipsis"),
    c("layer.new.fill", "fill"),
    c("layer.new.tone", "tone-ellipsis"),
    c("layer.new.paper", "paper-2"),
    c("layer.new.3d", "n-3d-layer"),
    c("layer.new.figure", "n-3d-drawing-figure"),
    c("layer.new.hand", "n-3d-hand"),
    c("layer.new.head", "n-3d-head"),
    c("layer.new.frame", "frame-border-folder-ellipsis"),
    c(
        "layer.correction.brightness",
        "brightness-contrast-ellipsis",
    ),
    c("layer.correction.levels", "level-correction-ellipsis"),
    c("layer.correction.curve", "tone-curve-ellipsis"),
    c("layer.correction.hue", "hue-saturation-luminosity-ellipsis"),
    c("layer.correction.balance", "color-balance-ellipsis"),
    c("layer.correction.invert", "reverse-gradient"),
    c("layer.correction.posterize", "posterization-ellipsis"),
    c("layer.correction.binarize", "binarization-ellipsis"),
    c("layer.correction.gradient_map", "gradient-map-ellipsis"),
    c("layer.folder_insert", "create-folder-and-insert-layer").keys("Ctrl+G"),
    c("layer.new.folder", "new-layer-folder"),
    c("layer.ungroup", "ungroup-layer-folder").keys("Ctrl+Shift+G"),
    c("layer.from_selection", "new-layer-from-selection"),
    c("layer.duplicate", "duplicate-layer"),
    c("layer.delete", "delete-layer"),
    c("layer.delete_empty", "empty-layers"),
    c("layer.delete_hidden", "hidden-layers"),
    c("layer.hide_drafts", "hide-all-visible-draft-layers"),
    c("layer.mask.outside", "mask-outside-selection"),
    c("layer.mask.inside", "mask-selection"),
    c("layer.mask.delete", "delete-mask"),
    c("layer.mask.apply", "apply-mask-to-layer"),
    c("layer.mask.toggle", "enable-mask"),
    c("layer.mask.edit", "edit-mask"),
    c("layer.ruler.delete", "delete-ruler"),
    c("layer.ruler.show", "show-ruler"),
    c(
        "layer.ruler.perspective",
        "create-perspective-ruler-ellipsis",
    ),
    c("layer.ruler.symmetry", "create-symmetrical-ruler"),
    c("layer.ruler.circle", "create-concentric-circle-ruler"),
    c("layer.toggle.visible", "show-layer"),
    c("layer.rename", "change-layer-name-ellipsis"),
    c("layer.toggle.clip", "clip-to-layer-below").keys("Ctrl+Alt+G"),
    c("layer.toggle.reference", "set-as-reference-layer"),
    c("layer.toggle.draft", "set-as-draft-layer"),
    c("layer.toggle.lock", "lock-layer"),
    c("layer.toggle.lock_alpha", "lock-transparent-pixels"),
    c("layer.properties", "layer-property"),
    c("layer.comps", "layer-comps-ellipsis"),
    c("layer.select_above", "select-layer-above").keys("Alt+]"),
    c("layer.select_below", "select-layer-below").keys("Alt+["),
    c("layer.select_all", "select-all-layers"),
    c("layer.select_none", "deselect-layers"),
    c("layer.order.front", "bring-to-front"),
    c("layer.order.up", "move-up").keys("Ctrl+]"),
    c("layer.order.down", "move-down").keys("Ctrl+["),
    c("layer.order.back", "send-to-back"),
    c("layer.transfer_down", "transfer-to-lower-layer"),
    c("layer.merge_down", "merge-with-layer-below").keys("Ctrl+E"),
    c("layer.merge_selected", "merge-selected-layers").keys("Alt+Shift+E"),
    c("layer.merge_visible_here", "merge-visible-layers").keys("Ctrl+Shift+E"),
    c("layer.merge_visible", "merge-visible-to-new-layer"),
    c("layer.flatten", "flatten-image"),
    c("layer.rasterize", "rasterize"),
    c("layer.convert", "convert-layer-ellipsis"),
    c(
        "layer.lines_tones_vector",
        "convert-to-vector-lines-and-tones",
    ),
];
