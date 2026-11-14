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

    /// Flips a property of the current layer.
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
        Self::apply(cx, doc::set(layer, key, !now));
    }
}
