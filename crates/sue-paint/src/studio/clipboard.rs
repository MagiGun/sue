//! Copy and paste, and pasting a material.

use sue_core::txn::SpawnEntity;
use sue_core::{CHILD, Entity, PixelFormat, RasterTile2D, Relation};
use sue_model::class;
use sue_model::picture;
use sue_wm::AppletContext;

use super::{Clip, SuePaint};
use crate::doc;

impl SuePaint {
    /// Puts `material` on the canvas as a new layer above the current one,
    /// in the drawing color.
    pub(crate) fn paste_material(
        &mut self,
        cx: &mut AppletContext<'_>,
        material: &crate::materials::Material,
    ) {
        if material.placement == crate::materials::Placement::Pose {
            self.apply_pose_material(cx, material);
            return;
        }
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
        let Some(raster) = crate::materials::layer(material, size, self.drawing_color()) else {
            cx.fail(tr!("cannot-read-the-material", name = material.name));
            return;
        };
        match doc::add_layer_pixels(
            &cx.world.arena,
            picture,
            self.layer(),
            material.name.clone(),
            raster,
        ) {
            Ok((spawn, id)) => {
                if Self::apply(
                    cx,
                    doc::Edit::new(tr!("paste-2", name = material.name)).then(spawn),
                ) && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(id);
                }
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Copies the selected pixels of the current layer.
    pub(crate) fn copy(&mut self, cx: &mut AppletContext<'_>) -> bool {
        let arena = &cx.world.arena;
        let Some(info) = self.layer_info(arena) else {
            return false;
        };
        let Some(raster) = doc::raster(arena, info.id) else {
            cx.fail(tr!("only-raster-layers-can-copied"));
            return false;
        };
        let Some(selection) = self.selection_or_all(arena) else {
            return false;
        };
        let Some([x0, y0, x1, y1]) = selection.bounds() else {
            return false;
        };
        let (width, height) = (x1 - x0, y1 - y0);
        let mut pixels = vec![0_u8; (width * height * 4) as usize];
        let Some(bytes) = raster.bytes() else {
            return false;
        };
        let [ox, oy] = info.style.offset;
        for y in 0..height {
            for x in 0..width {
                let (lx, ly) = (i64::from(x0 + x) - ox, i64::from(y0 + y) - oy);
                if lx < 0
                    || ly < 0
                    || lx >= i64::from(raster.width())
                    || ly >= i64::from(raster.height())
                {
                    continue;
                }
                let from = ((ly as u32 * raster.width() + lx as u32) * 4) as usize;
                let to = ((y * width + x) * 4) as usize;
                pixels[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
                let cover = selection.at(x0 + x, y0 + y);
                pixels[to + 3] = (f32::from(pixels[to + 3]) * cover).round() as u8;
            }
        }
        match RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels) {
            Ok(raster) => {
                self.clipboard = Some(Clip {
                    raster,
                    origin: [x0, y0],
                });
                true
            }
            Err(_) => false,
        }
    }

    /// Pastes the copied pixels as a new layer above the current one.
    pub(crate) fn paste(&mut self, cx: &mut AppletContext<'_>, name: &str) {
        let (Some(picture), Some(clip)) = (self.picture(), self.clipboard.clone()) else {
            return;
        };
        let arena = &cx.world.arena;
        let Some([width, height]) = arena.get_by_id(picture).ok().and_then(picture::size) else {
            return;
        };
        let mut pixels = vec![0_u8; (width * height * 4) as usize];
        let source = clip.raster.bytes().unwrap_or_default();
        for y in 0..clip.raster.height() {
            for x in 0..clip.raster.width() {
                let (px, py) = (clip.origin[0] + x, clip.origin[1] + y);
                if px >= width || py >= height {
                    continue;
                }
                let from = ((y * clip.raster.width() + x) * 4) as usize;
                let to = ((py * width + px) * 4) as usize;
                pixels[to..to + 4].copy_from_slice(&source[from..from + 4]);
            }
        }
        let Ok(raster) = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels) else {
            return;
        };
        let layer = Entity::new(name, raster).with_class(class::LAYER);
        let id = layer.id();
        let order = {
            let below = self.layer();
            let stack = doc::layers(arena, picture);
            let index = below.and_then(|below| stack.iter().position(|info| info.id == below));
            match index {
                Some(index) => sue_core::OrderKey::between(
                    Some(&stack[index].order),
                    stack.get(index + 1).map(|info| &info.order),
                ),
                None => arena.relations().next_order(picture, CHILD),
            }
        };
        let spawn =
            SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order));
        if Self::apply(cx, spawn)
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(id);
        }
    }
}
