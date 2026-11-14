//! Small pictures: the whole picture, or one layer, over the
//! checkerboard, and the signature that says when they are out of date.

use sue_core::{Arena, ComponentKind, EntityId};
use sue_model::picture::{self, LayerKind, LayerStyle};

use super::RenderOptions;
use super::gather::{Layer, Source, gather};
use super::rows::{checker, pixel, rows};

/// The picture's and its layers' revisions, in stacking order: when this
/// changes, the picture looks different.
#[must_use]
pub fn signature(arena: &Arena, picture: EntityId) -> Vec<(EntityId, u64)> {
    let revision = |id| {
        arena
            .get_by_id(id)
            .map_or(u64::MAX, sue_core::Entity::revision)
    };
    let mut seen = vec![(picture, revision(picture))];
    for id in picture::layers(arena, picture) {
        seen.push((id, revision(id)));
        // A mask's edits change how its layer looks.
        if let Some(mask) = crate::doc::mask_of(arena, id) {
            seen.push((mask, revision(mask)));
        }
    }
    seen
}

/// The whole picture at most `max` pixels a side (sampled, not filtered),
/// over the checkerboard: opaque RGBA8 pixels and their size.
#[must_use]
pub fn preview(arena: &Arena, picture: EntityId, max: [u32; 2]) -> Option<(Vec<u8>, [u32; 2])> {
    let size = picture::size(arena.get_by_id(picture).ok()?)?;
    let scale = (max[0] as f32 / size[0] as f32)
        .min(max[1] as f32 / size[1] as f32)
        .min(1.0);
    let out = [
        ((size[0] as f32 * scale).round() as u32).max(1),
        ((size[1] as f32 * scale).round() as u32).max(1),
    ];
    let layers = gather(arena, picture, RenderOptions::default());
    let mut pixels = vec![0_u8; out[0] as usize * out[1] as usize * 4];
    rows(
        &mut pixels,
        out[0] as usize * 4,
        0,
        (out[0] * out[1]) as usize,
        |ty, row| {
            for (tx, slot) in (0..out[0]).zip(row.as_chunks_mut::<4>().0) {
                let x = (((tx as f32 + 0.5) / scale) as u32).min(size[0] - 1);
                let y = (((ty as f32 + 0.5) / scale) as u32).min(size[1] - 1);
                let color = pixel(&layers, x, y);
                let under = checker(tx, ty);
                for (channel, value) in slot.iter_mut().zip(color) {
                    let shown = value * color[3] + under * (1.0 - color[3]);
                    *channel = (shown.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                slot[3] = 255;
            }
        },
    );
    Some((pixels, out))
}

/// A small preview of one layer, `max` pixels at most a side, over the
/// checkerboard: RGBA8 pixels and their size.
#[must_use]
pub fn thumbnail(arena: &Arena, layer: EntityId, max: [u32; 2]) -> Option<(Vec<u8>, [u32; 2])> {
    let entity = arena.get_by_id(layer).ok()?;
    let picture_size = picture::picture_of(arena, layer)
        .and_then(|picture| arena.get_by_id(picture).ok())
        .and_then(picture::size);
    let (source, size) = match LayerKind::of(entity) {
        LayerKind::Raster => {
            let raster = entity.component(ComponentKind::RasterTile2D)?.as_raster()?;
            let size = picture_size.unwrap_or([raster.width(), raster.height()]);
            (
                Source::Raster {
                    raster,
                    offset: LayerStyle::of(entity).offset,
                },
                size,
            )
        }
        LayerKind::Paper => (
            Source::Paper(
                picture::paper_color(entity)
                    .to_space(sue_core::ColorSpace::Srgb)
                    .rgb,
            ),
            picture_size.unwrap_or([4, 3]),
        ),
        LayerKind::Other => return None,
    };
    let layer = Layer::new(source, LayerStyle::default());
    let scale = (max[0] as f32 / size[0].max(1) as f32).min(max[1] as f32 / size[1].max(1) as f32);
    let out = [
        ((size[0] as f32 * scale).round() as u32).max(1),
        ((size[1] as f32 * scale).round() as u32).max(1),
    ];
    let mut pixels = Vec::with_capacity(out[0] as usize * out[1] as usize * 4);
    for ty in 0..out[1] {
        for tx in 0..out[0] {
            let x = ((tx as f32 + 0.5) / scale) as u32;
            let y = ((ty as f32 + 0.5) / scale) as u32;
            let color = layer.sample(x.min(size[0] - 1), y.min(size[1] - 1));
            let under = if ((tx / 4) + (ty / 4)).is_multiple_of(2) {
                1.0
            } else {
                0.8
            };
            for value in &color[..3] {
                let shown = value * color[3] + under * (1.0 - color[3]);
                pixels.push((shown.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            pixels.push(255);
        }
    }
    Some((pixels, out))
}
