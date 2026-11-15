//! The tiles that were already made, kept as textures.
//!
//! A tile is made the first time an icon is drawn in a style, at a size
//! in pixels and with a pen of a width, and kept in the egui context for
//! as long as the context lives. Drawing an icon again is then a lookup
//! and one quad. Nothing here needs a GPU: the context holds the pixels
//! and hands them to whichever painter draws the frame, so the cache
//! works the same in the tests that run without a window.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use egui::{Color32, ColorImage, Context, Id, TextureHandle, TextureId, TextureOptions};

use super::pack::{self, IconSet};
use super::{Icon, IconStyle};

/// The most tiles kept. When one more is needed they are all dropped
/// and made again as they are drawn, which only an interface that keeps
/// changing its scale ever sees.
const MOST: usize = 1024;

/// What makes one tile different from another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    /// The set of drawings it was made from ([`IconSet::key`]): 0 for
    /// Sue's own.
    set: u64,
    icon: Icon,
    style: IconStyle,
    /// The side of the tile, in pixels.
    pixels: u32,
    /// The width of the pen, in 64ths of a pixel.
    stroke: u32,
}

/// The tiles of one egui context.
#[derive(Clone, Default)]
struct Tiles(Arc<Mutex<HashMap<Key, TextureHandle>>>);

fn tiles(ctx: &Context) -> Tiles {
    ctx.data_mut(|data| data.get_temp_mut_or_default::<Tiles>(Id::NULL).clone())
}

/// The texture of `icon` in `style`, `pixels` wide, drawn with a pen
/// `stroke` pixels wide, from `set` or else from the set `ctx` shows
/// ([`pack::show`]): made now if it was never drawn like that before.
pub(super) fn texture(
    ctx: &Context,
    set: Option<&IconSet>,
    icon: Icon,
    style: IconStyle,
    pixels: usize,
    stroke: f32,
) -> TextureId {
    // An icon without a solid twin has one tile for both styles.
    let style = if icon.has_solid() {
        style
    } else {
        IconStyle::Line
    };
    let showing;
    let set = match set {
        Some(set) => set,
        None => {
            showing = pack::showing(ctx);
            showing.as_ref()
        }
    };
    let key = Key {
        set: set.key(),
        icon,
        style,
        pixels: pixels as u32,
        stroke: (stroke * 64.0).round() as u32,
    };
    let tiles = tiles(ctx);
    let mut held = tiles.0.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(texture) = held.get(&key) {
        return texture.id();
    }
    if held.len() >= MOST {
        held.clear();
    }
    let tile = set.tile(icon, style, pixels, key.stroke as f32 / 64.0);
    // White with the coverage as its opacity, so that the tint a quad
    // is drawn with becomes the color of the icon.
    let image = ColorImage::new(
        [tile.size, tile.size],
        tile.alpha
            .iter()
            .map(|alpha| Color32::from_white_alpha(*alpha))
            .collect(),
    );
    let name = format!(
        "sue-icon {} {} {pixels} {}",
        icon.name(),
        style.name(),
        key.set
    );
    // The quad covers exactly the tile's pixels, where linear sampling
    // gives each pixel its own value; only an icon larger than the
    // largest tile is really scaled.
    let texture = ctx.load_texture(name, image, TextureOptions::LINEAR);
    let id = texture.id();
    held.insert(key, texture);
    id
}

/// Drops every tile `ctx` holds, so that each is made again from the
/// set it shows now.
pub(super) fn forget(ctx: &Context) {
    tiles(ctx)
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
}

/// How many tiles `ctx` holds.
#[must_use]
pub fn cached(ctx: &Context) -> usize {
    tiles(ctx)
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .len()
}

#[cfg(test)]
mod tests {
    use egui::{Rect, pos2, vec2};

    use super::super::headless::{self, Seen};
    use super::super::{Icon, IconStyle, paint, paint_styled, weight};
    use super::*;

    /// One small frame at `scale` pixels per point.
    fn frame(ctx: &Context, scale: f32, draw: impl Fn(&egui::Painter)) -> Seen {
        headless::frame(ctx, [200.0, 100.0], scale, |painter, _| draw(painter))
    }

    /// The textured quads of a frame: texture, corners in points, color.
    fn quads(ctx: &Context, output: &Seen) -> Vec<(TextureId, Rect, Color32)> {
        ctx.tessellate(output.shapes.clone(), output.pixels_per_point)
            .into_iter()
            .filter_map(|clipped| match clipped.primitive {
                egui::epaint::Primitive::Mesh(mesh)
                    if mesh.texture_id != TextureId::default() && mesh.vertices.len() == 4 =>
                {
                    let mut rect = Rect::NOTHING;
                    for vertex in &mesh.vertices {
                        rect.extend_with(vertex.pos);
                    }
                    Some((mesh.texture_id, rect, mesh.vertices[0].color))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_tile_is_made_once_and_then_only_looked_up() {
        let ctx = Context::default();
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(24.0, 24.0));
        let draw = |painter: &egui::Painter| paint(painter, rect, Icon::Search, Color32::WHITE);
        assert_eq!(cached(&ctx), 0);
        let first = frame(&ctx, 1.0, draw);
        assert_eq!(cached(&ctx), 1);
        let made: Vec<_> = first
            .sets
            .iter()
            .filter(|(_, delta)| delta.image.size() == [24, 24])
            .collect();
        assert_eq!(
            made.len(),
            1,
            "one 24 pixel texture is handed to the painter"
        );
        let (id, delta) = made[0];
        let id = *id;
        assert!(delta.pos.is_none(), "a whole texture, not a patch");
        // The pixels are the tile: white, as opaque as the icon covers.
        let tile = Icon::Search.tile(IconStyle::Line, 24, weight(24.0));
        let egui::ImageData::Color(image) = &delta.image;
        for (pixel, alpha) in image.pixels.iter().zip(&tile.alpha) {
            assert_eq!(*pixel, Color32::from_white_alpha(*alpha));
        }
        let drawn = quads(&ctx, &first);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].0, id);
        // The next frames draw the same texture and make nothing.
        for _ in 0..3 {
            let again = frame(&ctx, 1.0, draw);
            assert_eq!(cached(&ctx), 1);
            assert!(
                again
                    .sets
                    .iter()
                    .all(|(_, delta)| delta.image.size() != [24, 24])
            );
            assert_eq!(again.freed, 0);
            assert_eq!(quads(&ctx, &again)[0].0, id);
        }
    }

    #[test]
    fn each_icon_style_size_and_scale_has_its_own_tile() {
        let ctx = Context::default();
        let at = |side: f32| Rect::from_min_size(pos2(10.0, 10.0), vec2(side, side));
        let _ = frame(&ctx, 1.0, |painter| {
            paint(painter, at(24.0), Icon::Search, Color32::WHITE);
            paint(painter, at(24.0), Icon::Folder, Color32::WHITE);
            paint(painter, at(16.0), Icon::Search, Color32::WHITE);
            paint_styled(
                painter,
                at(24.0),
                Icon::Heart,
                IconStyle::Line,
                Color32::WHITE,
            );
            paint_styled(
                painter,
                at(24.0),
                Icon::Heart,
                IconStyle::Solid,
                Color32::WHITE,
            );
            // No solid twin: the same tile as its line. Another color:
            // the same tile again.
            paint_styled(
                painter,
                at(24.0),
                Icon::Search,
                IconStyle::Solid,
                Color32::RED,
            );
        });
        assert_eq!(cached(&ctx), 5);
        // A display scaled to one and a half: 36 pixels for 24 points.
        let scaled = frame(&ctx, 1.5, |painter| {
            paint(painter, at(24.0), Icon::Search, Color32::WHITE);
        });
        assert_eq!(cached(&ctx), 6);
        assert!(
            scaled
                .sets
                .iter()
                .any(|(_, delta)| delta.image.size() == [36, 36])
        );
    }

    #[test]
    fn the_quad_lies_on_whole_pixels_and_carries_the_tint() {
        let ctx = Context::default();
        let tint = Color32::from_rgb(40, 120, 200);
        for scale in [1.0_f32, 1.25, 1.5, 2.0] {
            for (corner, side) in [
                (pos2(10.3, 7.6), 24.0),
                (pos2(3.5, 3.5), 16.0),
                (pos2(50.1, 20.9), 20.0),
            ] {
                // A rect that is not square: the icon takes the shorter
                // side and sits in the middle.
                let rect = Rect::from_min_size(corner, vec2(side + 9.0, side));
                let output = frame(&ctx, scale, |painter| {
                    paint(painter, rect, Icon::Folder, tint);
                });
                let drawn = quads(&ctx, &output);
                assert_eq!(drawn.len(), 1);
                let (_, quad, color) = drawn[0];
                assert_eq!(color, tint);
                let pixels = (side * scale).round();
                for edge in [quad.left(), quad.top(), quad.right(), quad.bottom()] {
                    let pixel = edge * scale;
                    assert!((pixel - pixel.round()).abs() < 1e-3, "{scale} {edge}");
                }
                assert!((quad.width() * scale - pixels).abs() < 1e-3);
                assert!((quad.height() * scale - pixels).abs() < 1e-3);
                assert!((quad.center().x - rect.center().x).abs() * scale <= 0.5 + 1e-3);
                assert!((quad.center().y - rect.center().y).abs() * scale <= 0.5 + 1e-3);
            }
        }
    }

    #[test]
    fn nothing_is_drawn_into_no_room_or_in_no_color() {
        let ctx = Context::default();
        let output = frame(&ctx, 1.0, |painter| {
            let nothing = Rect::from_min_size(pos2(5.0, 5.0), vec2(0.0, 24.0));
            paint(painter, nothing, Icon::Search, Color32::WHITE);
            paint(painter, Rect::NOTHING, Icon::Search, Color32::WHITE);
            let room = Rect::from_min_size(pos2(5.0, 5.0), vec2(24.0, 24.0));
            paint(painter, room, Icon::Search, Color32::TRANSPARENT);
        });
        assert_eq!(cached(&ctx), 0);
        assert!(quads(&ctx, &output).is_empty());
    }

    #[test]
    fn a_very_large_icon_is_drawn_from_the_largest_tile() {
        let ctx = Context::default();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(700.0, 700.0));
        let output = frame(&ctx, 1.0, |painter| {
            paint(painter, rect, Icon::Star, Color32::WHITE);
        });
        assert!(
            output
                .sets
                .iter()
                .any(|(_, delta)| delta.image.size() == [512, 512])
        );
        let drawn = quads(&ctx, &output);
        assert_eq!(drawn[0].1.width(), 700.0);
    }

    #[test]
    fn the_cache_is_emptied_when_it_is_full() {
        let ctx = Context::default();
        let _ = frame(&ctx, 1.0, |_| {});
        // Every pen width is a tile of its own.
        for step in 0..=MOST {
            let _ = texture(
                &ctx,
                None,
                Icon::Info,
                IconStyle::Line,
                8,
                1.0 + step as f32 / 64.0,
            );
        }
        assert_eq!(cached(&ctx), 1);
    }
}
