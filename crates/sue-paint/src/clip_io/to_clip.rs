//! A picture of Sue's into a [`ClipDocument`]: every folder and layer as
//! it stacks, in its folder, with its style, pixels and mask. A layer
//! imported from a `.clip` and not changed since (its fingerprint is the
//! one its `clip.foreign` component was made with) goes back exactly as
//! it was read, with everything Sue does not map; a changed one is
//! written from Sue's model and its kept data is left out. What the file
//! held beyond its layers comes back while the canvas keeps its size.

use sue_core::{Arena, ComponentKind, Entity, EntityId, PixelFormat, RasterTile2D};
use sue_io::clip::{ClipBitmap, ClipDocument, ClipLayer, ClipMask};
use sue_model::picture::{self, LayerKind};
use sue_model::{BlendMode, Foreign};

use super::report::{Line, Outcome, Report};
use super::{FORMAT, composite_of, keep, print};
use crate::doc;

/// The kept data of `part` on `entity`, when it is the `.clip`
/// converter's.
fn kept<'a>(entity: &'a Entity, part: &str) -> Option<&'a Foreign> {
    entity
        .ext::<Foreign>()
        .filter(|foreign| foreign.format == FORMAT && foreign.part == part)
}

/// `value` 0 to 1 as 0 to 255.
fn unit_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A raster's pixels as straight RGBA8.
fn rgba(raster: &RasterTile2D) -> Option<Vec<u8>> {
    match raster.format() {
        PixelFormat::Rgba8 => raster.bytes().map(<[u8]>::to_vec),
        PixelFormat::Gray8 => raster
            .bytes()
            .map(|bytes| bytes.iter().flat_map(|v| [*v, *v, *v, 255]).collect()),
        PixelFormat::RgbaF32 => raster
            .f32_samples()
            .map(|samples| samples.iter().map(|s| unit_byte(*s)).collect()),
    }
}

struct Writing<'a> {
    arena: &'a Arena,
    picture: EntityId,
    size: [u32; 2],
    report: Report,
}

impl Writing<'_> {
    /// The layers whose folder is `parent`, bottom first.
    fn layers(&mut self, stack: &[doc::LayerInfo], parent: Option<EntityId>) -> Vec<ClipLayer> {
        let mut out = Vec::new();
        for info in stack
            .iter()
            .filter(|info| doc::folder_of(self.arena, info.id) == parent)
        {
            let Ok(entity) = self.arena.get_by_id(info.id) else {
                continue;
            };
            let folder = doc::is_folder(self.arena, info.id);
            let children = if folder {
                self.layers(stack, Some(info.id))
            } else {
                Vec::new()
            };
            let mask =
                doc::mask_of(self.arena, info.id).and_then(|mask| self.arena.get_by_id(mask).ok());
            let now = print::layer(entity, mask);
            if let Some(foreign) = kept(entity, "layer").filter(|foreign| foreign.unchanged(&now))
                && let Ok(mut layer) = keep::layer_from(&foreign.data)
            {
                layer.children = children;
                self.report.lines.push(Line {
                    name: info.name.clone(),
                    outcome: Outcome::Exact,
                });
                out.push(layer);
                continue;
            }
            let made = if folder {
                Some((self.folder(info, children), Outcome::Exact))
            } else {
                self.layer(entity, info)
            };
            let Some((mut layer, outcome)) = made else {
                self.report.lines.push(Line {
                    name: info.name.clone(),
                    outcome: Outcome::LeftOut,
                });
                continue;
            };
            if let Some(mask) = self.mask(info, &layer) {
                layer.mask = Some(mask);
                layer.set_mask_enabled(doc::mask_on(self.arena, info.id));
            }
            self.report.lines.push(Line {
                name: info.name.clone(),
                outcome,
            });
            out.push(layer);
        }
        out
    }

    fn style(&self, info: &doc::LayerInfo, layer: &mut ClipLayer) {
        layer.opacity = (f64::from(info.style.opacity.clamp(0.0, 1.0)) * 256.0).round() as i64;
        layer.set_visible(info.style.visible);
        layer.clip = i64::from(info.style.clip);
        layer.draft = info.style.draft;
        layer.reference = doc::is_reference(self.arena, info.id);
    }

    fn folder(&self, info: &doc::LayerInfo, children: Vec<ClipLayer>) -> ClipLayer {
        let mut layer = ClipLayer::folder(&info.name);
        layer.set_open(doc::is_open(self.arena, info.id));
        layer.composite = composite_of(doc::folder_blend(self.arena, info.id));
        self.style(info, &mut layer);
        layer.children = children;
        layer
    }

    /// A layer that is not a folder, and how it came out; `None` for one
    /// Clip Studio has nothing for.
    fn layer(&self, entity: &Entity, info: &doc::LayerInfo) -> Option<(ClipLayer, Outcome)> {
        let [width, height] = self.size;
        let (mut layer, outcome) = match LayerKind::of(entity) {
            LayerKind::Paper => {
                let rgb = picture::paper_color(entity)
                    .to_space(sue_core::ColorSpace::Srgb)
                    .rgb;
                let color = rgb.map(unit_byte);
                (
                    ClipLayer::paper(&info.name, color, width, height),
                    Outcome::Exact,
                )
            }
            LayerKind::Raster => {
                let raster = entity.component(ComponentKind::RasterTile2D)?.as_raster()?;
                let pixels = rgba(raster)?;
                let [x, y] = info.style.offset;
                let layer = ClipLayer::raster(
                    &info.name,
                    x,
                    y,
                    ClipBitmap::color(raster.width(), raster.height(), &pixels),
                );
                // Text, vector lines, frames and the like ride on a raster:
                // Clip Studio gets what they drew.
                let more = entity.exts().any(|component| {
                    component.type_name() != <Foreign as sue_core::ComponentType>::NAME
                });
                let outcome = if more {
                    Outcome::Flattened
                } else if raster.format() == PixelFormat::Rgba8 {
                    Outcome::Exact
                } else {
                    Outcome::Approximated
                };
                (layer, outcome)
            }
            LayerKind::Other => return None,
        };
        let blend = info.style.blend;
        layer.composite = composite_of(if blend == BlendMode::Through {
            BlendMode::Normal
        } else {
            blend
        });
        self.style(info, &mut layer);
        Some((layer, outcome))
    }

    /// `layer`'s mask, as a one channel bitmap where it sits.
    fn mask(&self, info: &doc::LayerInfo, layer: &ClipLayer) -> Option<ClipMask> {
        let mask = doc::mask_of(self.arena, info.id)?;
        let raster = doc::raster(self.arena, mask)?;
        let at = doc::mask_offset(self.arena, info.id)?;
        let (w, h) = (raster.width(), raster.height());
        let mut values = Vec::with_capacity(w as usize * h as usize);
        for y in 0..i64::from(h) {
            for x in 0..i64::from(w) {
                values.push(unit_byte(doc::cover(raster, x, y)));
            }
        }
        Some(ClipMask {
            offset: [at[0] - layer.offset[0], at[1] - layer.offset[1]],
            offscreen_offset: [0, 0],
            bitmap: ClipBitmap::gray(w, h, &values, false),
        })
    }
}

/// `picture` as a [`ClipDocument`], and what became of each layer.
///
/// # Errors
/// When the picture has no size.
pub fn document(arena: &Arena, picture: EntityId) -> Result<(ClipDocument, Report), String> {
    let entity = arena
        .get_by_id(picture)
        .map_err(|_| tr!("this-picture-has-no-size"))?;
    let [width, height] = picture::size(entity).ok_or_else(|| tr!("this-picture-has-no-size"))?;
    let dpi = entity
        .property(picture::DPI)
        .and_then(sue_core::Value::as_f64)
        .unwrap_or(72.0);
    let mut writing = Writing {
        arena,
        picture,
        size: [width, height],
        report: Report::default(),
    };
    let kept_canvas = kept(entity, "picture");
    let mut doc = match kept_canvas
        .filter(|foreign| foreign.unchanged(&print::picture(entity)))
        .and_then(|foreign| keep::picture_from(&foreign.data).ok())
    {
        Some(doc) => doc,
        None => {
            writing.report.canvas_dropped = kept_canvas.is_some();
            ClipDocument::new(width, height, dpi)
        }
    };
    doc.dpi = dpi;
    let stack = doc::layers(arena, writing.picture);
    doc.root.children = writing.layers(&stack, None);
    // Sue's own style is in the layers now: no hidden copy of the project.
    doc.sue = None;
    Ok((doc, writing.report))
}
