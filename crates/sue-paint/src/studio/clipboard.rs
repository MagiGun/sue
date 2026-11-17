//! Copy and paste, and pasting a material.

use sue_core::{PixelFormat, RasterTile2D};
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
        // A picture registered with its own paste settings (FGV-48):
        // scaled, tiled or not, its layer at the top or above the current.
        let props = match &material.source {
            crate::materials::Source::File(path) => {
                crate::material_props::MaterialProps::read(path)
            }
            crate::materials::Source::Builtin(_) => None,
        }
        .filter(|_| material.placement == crate::materials::Placement::Fit);
        let raster = match &props {
            Some(props) => crate::materials::picture(material, size, self.drawing_color())
                .and_then(|tile| props.paste(&tile, size)),
            None => crate::materials::layer(material, size, self.drawing_color()),
        };
        let Some(raster) = raster else {
            cx.fail(tr!("cannot-read-the-material", name = material.name));
            return;
        };
        let above = match props.map(|props| props.order) {
            Some(crate::material_props::Order::Top) => doc::layers(&cx.world.arena, picture)
                .last()
                .map(|info| info.id),
            _ => self.layer(),
        };
        match doc::add_layer_pixels(
            &cx.world.arena,
            picture,
            above,
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
        // In the picked folder, or right above the picked layer in its
        // folder, as every new layer goes.
        let Ok((spawn, id)) =
            doc::add_layer_pixels(arena, picture, self.layer(), name.to_owned(), raster)
        else {
            return;
        };
        if Self::apply(cx, spawn)
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(id);
        }
    }
}
