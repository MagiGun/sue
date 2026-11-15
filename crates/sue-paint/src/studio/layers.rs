//! Moving, aligning and toggling layers.

use sue_core::{EntityId, Value};
use sue_model::picture;
use sue_wm::{AppletContext, ArenaEdit};

use super::SuePaint;
use crate::doc;

impl SuePaint {
    /// Edit, Align/Distribute: moves the current layer so its drawn
    /// pixels line up with the canvas's `edge` (left, hcenter, right, top,
    /// vcenter or bottom), one undo step.
    pub(crate) fn align_layer(&mut self, edge: &str, cx: &mut AppletContext<'_>) {
        let Some(info) = self.layer_info(&cx.world.arena) else {
            return;
        };
        let Some(size) = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .and_then(picture::size)
        else {
            return;
        };
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        let Some(bytes) = raster.bytes() else {
            return;
        };
        let (width, height) = (raster.width() as usize, raster.height() as usize);
        let mut bounds: Option<[usize; 4]> = None;
        for y in 0..height {
            for x in 0..width {
                if bytes[(y * width + x) * 4 + 3] > 0 {
                    let b = bounds.get_or_insert([x, y, x, y]);
                    b[0] = b[0].min(x);
                    b[1] = b[1].min(y);
                    b[2] = b[2].max(x);
                    b[3] = b[3].max(y);
                }
            }
        }
        let Some([left, top, right, bottom]) = bounds else {
            cx.notify(tr!("nothing-drawn-to-align"));
            return;
        };
        let [ox, oy] = info.style.offset;
        let (left, top) = (left as i64 + ox, top as i64 + oy);
        let (right, bottom) = (right as i64 + 1 + ox, bottom as i64 + 1 + oy);
        let (w, h) = (i64::from(size[0]), i64::from(size[1]));
        let (key, by) = match edge {
            "left" => (picture::X, -left),
            "hcenter" => (picture::X, (w - (right - left)) / 2 - left),
            "right" => (picture::X, w - right),
            "top" => (picture::Y, -top),
            "vcenter" => (picture::Y, (h - (bottom - top)) / 2 - top),
            _ => (picture::Y, h - bottom),
        };
        if by != 0 {
            let now = if key == picture::X { ox } else { oy };
            Self::apply(cx, doc::set(info.id, key, now + by));
        }
    }

    /// Deletes the vector lines the Object tool picked, as one step.
    pub(crate) fn delete_picked_lines(&mut self, cx: &mut AppletContext<'_>) {
        let Some((layer, picked)) = self.picked_lines.take() else {
            return;
        };
        let Some(before) = doc::raster(&cx.world.arena, layer)
            .and_then(|raster| raster.bytes())
            .map(<[u8]>::to_vec)
        else {
            return;
        };
        let geometry = crate::vector::geometry(&cx.world.arena, layer);
        let lines: Vec<crate::vector::Line> = crate::vector::lines(&geometry)
            .into_iter()
            .enumerate()
            .filter(|(index, _)| !picked.contains(index))
            .map(|(_, line)| line)
            .collect();
        let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer) else {
            return;
        };
        let (width, height) = (raster.width(), raster.height());
        if let Some(painted) = crate::vector::render(&lines, width, height)
            && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
        {
            into.copy_from_slice(from);
        }
        let pixels = doc::raster(&cx.world.arena, layer).and_then(|raster| {
            let patch = doc::diff_patch(&before, raster.bytes()?, width, height)?;
            Some(sue_core::txn::RasterEdit::new(
                layer,
                tr!("delete-lines"),
                vec![patch],
            ))
        });
        let edit = crate::vector::keep(
            &mut cx.world.arena,
            layer,
            geometry,
            &lines,
            pixels,
            &tr!("delete-lines"),
        );
        cx.record(ArenaEdit(edit));
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// Moves the current layer to `index` (0 is the bottom).
    /// Moves the current layer up or down as Clip Studio Paint's layer
    /// palette does with folders: a folder moves with everything in it, a
    /// layer leaves its folder past the folder's top or bottom and joins a
    /// folder it moves into. Returns `false` when a plain move does.
    pub(crate) fn move_in_folders(&mut self, cx: &mut AppletContext<'_>, up: bool) -> bool {
        let (Some(picture), Some(layer)) = (self.picture(), self.layer()) else {
            return false;
        };
        let arena = &cx.world.arena;
        let stack: Vec<EntityId> = doc::layers(arena, picture)
            .into_iter()
            .map(|info| info.id)
            .collect();
        let Some(at) = stack.iter().position(|id| *id == layer) else {
            return false;
        };
        if doc::is_folder(arena, layer) {
            // The folder and its layers move as one block, past one layer.
            let members = doc::members(arena, picture, layer);
            let low = stack
                .iter()
                .position(|id| members.contains(id))
                .map_or(at, |first| first.min(at));
            let neighbor = if up {
                stack.get(at + 1)
            } else {
                low.checked_sub(1).and_then(|below| stack.get(below))
            };
            let Some(&neighbor) = neighbor else {
                return true;
            };
            let to = if up { low } else { at };
            if let Some(edit) = doc::reorder(arena, picture, neighbor, to) {
                Self::apply(cx, edit);
            }
            return true;
        }
        let folder = doc::folder_of(arena, layer);
        if up {
            let Some(&above) = stack.get(at + 1) else {
                return false;
            };
            if Some(above) == folder {
                // Out past the top of its folder.
                let edit = doc::reorder(arena, picture, layer, at + 1);
                let out = crate::anim::set_folder(arena, layer, doc::folder_of(arena, above));
                for edit in [edit, out].into_iter().flatten() {
                    Self::apply(cx, edit);
                }
                return true;
            }
            let joins = if doc::is_folder(arena, above) && doc::folder_of(arena, above) == folder {
                // Right under an empty folder.
                doc::members(arena, picture, above)
                    .is_empty()
                    .then_some(above)
            } else {
                doc::folder_of(arena, above).filter(|outer| {
                    Some(*outer) != folder && !doc::folders_around(arena, *outer).contains(&layer)
                })
            };
            if let Some(into) = joins
                && let Some(edit) = crate::anim::set_folder(arena, layer, Some(into))
            {
                // Into the folder from below, staying put.
                Self::apply(cx, edit);
                return true;
            }
            false
        } else {
            let below = at
                .checked_sub(1)
                .and_then(|index| stack.get(index))
                .copied();
            if let Some(folder) = folder
                && below.is_none_or(|below| !doc::folders_around(arena, below).contains(&folder))
            {
                // Out past the bottom of its folder, staying put.
                if let Some(edit) =
                    crate::anim::set_folder(arena, layer, doc::folder_of(arena, folder))
                {
                    Self::apply(cx, edit);
                }
                return true;
            }
            if let Some(below) = below
                && doc::is_folder(arena, below)
                && doc::folder_of(arena, below) == folder
            {
                // Down into the folder under it, as its top layer.
                let edit = doc::reorder(arena, picture, layer, at - 1);
                let into = crate::anim::set_folder(arena, layer, Some(below));
                for edit in [edit, into].into_iter().flatten() {
                    Self::apply(cx, edit);
                }
                return true;
            }
            false
        }
    }

    pub(crate) fn move_layer_to(&mut self, cx: &mut AppletContext<'_>, index: usize) {
        let (Some(picture), Some(layer)) = (self.picture(), self.layer()) else {
            return;
        };
        if let Some(relate) = doc::reorder(&cx.world.arena, picture, layer, index) {
            Self::apply(cx, relate);
        }
    }

    /// Flips a property of the current layer, and sets every picked layer
    /// the same way (Clip Studio Paint's lock, show and the other switches
    /// work on every picked layer), as one step.
    pub(crate) fn toggle_layer(&mut self, cx: &mut AppletContext<'_>, key: &str, default: bool) {
        let Some(layer) = self.layer() else {
            return;
        };
        let now = cx
            .world
            .arena
            .get_by_id(layer)
            .ok()
            .and_then(|entity| entity.property(key))
            .and_then(Value::as_bool)
            .unwrap_or(default);
        let ids = self.picked_layers(&cx.world.arena);
        if let Some(edit) = doc::set_layers(&cx.world.arena, &ids, key, !now) {
            Self::apply(cx, edit);
        }
    }

    /// The current layer and the layers picked with it, bottom first.
    pub(crate) fn picked_layers(&self, arena: &sue_core::Arena) -> Vec<EntityId> {
        let (Some(picture), Some(doc)) = (self.picture(), self.doc()) else {
            return Vec::new();
        };
        doc::layers(arena, picture)
            .into_iter()
            .map(|info| info.id)
            .filter(|id| doc.layer == Some(*id) || doc.picked.contains(id))
            .collect()
    }

    /// Delete Layer: the current layer and every picked one, a folder with
    /// what it holds and an animation folder with its tracks, as one step.
    /// While a folder is among them Sue asks first, unless that was turned
    /// off (`sue_paint.ask_delete_folders`) or `asked`.
    pub(crate) fn delete_layers(&mut self, cx: &mut AppletContext<'_>, asked: bool) {
        let Some(picture) = self.picture() else {
            return;
        };
        let ids = self.picked_layers(&cx.world.arena);
        let arena = &cx.world.arena;
        let folders = ids.iter().any(|id| doc::is_folder(arena, *id));
        if folders && !asked && cx.rice().get_bool(ASK_DELETE_FOLDERS).unwrap_or(true) {
            self.dialog = Some(crate::dialogs::Dialog::DeleteFolders(false));
            return;
        }
        let edit = match ids[..] {
            [] => return,
            [folder] if doc::is_folder(arena, folder) => {
                Ok(crate::anim::delete_folder(arena, picture, folder))
            }
            _ => doc::delete_layers(arena, picture, &ids).map(|edit| {
                match timeline_without(arena, picture, &doc::with_members(arena, picture, &ids)) {
                    Some(sequence) => {
                        edit.then(sue_core::txn::SetExtension::set(picture, sequence))
                    }
                    None => edit,
                }
            }),
        };
        let stack = doc::layers(arena, picture);
        let gone = doc::with_members(arena, picture, &ids);
        // What is current after: the layer under the lowest one deleted.
        let next = stack
            .iter()
            .position(|info| gone.contains(&info.id))
            .and_then(|lowest| stack[..lowest].last())
            .or_else(|| stack.iter().find(|info| !gone.contains(&info.id)))
            .map(|info| info.id);
        match edit {
            Ok(edit) => {
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = next;
                    doc.picked.clear();
                    doc.mask_edit = false;
                }
            }
            Err(error) => cx.fail(error),
        }
    }
}

impl SuePaint {
    /// Change Palette Color: the offered color named `name` (`none`: No
    /// Palette Color) on the current and every picked layer, one step.
    pub(crate) fn set_palette_color_named(&mut self, cx: &mut AppletContext<'_>, name: &str) {
        let color = doc::named_palette_color(name);
        if color.is_none() && name != "none" {
            return;
        }
        let ids = self.picked_layers(&cx.world.arena);
        if let Some(edit) = doc::set_palette_color(&cx.world.arena, &ids, color) {
            Self::apply(cx, edit);
        }
    }
}

/// Ask before deleting layer folders (`Bool`, on unless "Do not show
/// again" was ticked).
pub(crate) const ASK_DELETE_FOLDERS: &str = "sue_paint.ask_delete_folders";

/// `picture`'s timeline without the tracks that stand for any layer of
/// `gone` (a cel, camera or layer track), or `None` when none does.
fn timeline_without(
    arena: &sue_core::Arena,
    picture: EntityId,
    gone: &[EntityId],
) -> Option<sue_model::Sequence> {
    let sequence = crate::anim::timeline(arena, picture)?;
    let mut out = sequence.clone();
    out.tracks.retain(|track| {
        let stands_for = crate::anim::setup::track_layer(track).or_else(|| {
            (track.kind == crate::anim::CELS || track.kind == crate::anim::setup::CAMERA)
                .then(|| crate::anim::track_folder(track))
                .flatten()
        });
        stands_for.is_none_or(|id| !gone.contains(&id))
    });
    (out != *sequence).then_some(out)
}
