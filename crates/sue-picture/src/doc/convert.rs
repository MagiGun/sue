//! Changing a whole picture's size, resolution and orientation, and
//! turning a layer from one kind into another (Rasterize, Convert Layer).

use sue_core::txn::{Relate, SetProperty, SpawnEntity};
use sue_core::{Arena, CHILD, Entity, EntityId, PixelFormat, RasterTile2D, Relation, Value};
use sue_model::class;
use sue_model::picture::{self, LayerStyle};

use super::{
    Edit, MAX_SIDE, MEMBER, delete, folder_of, key_above, layers, mask_of, raster, size_of,
};

/// RGBA8 pixels resized to `to`: averaged over each new pixel's area when
/// smaller, bilinear when larger, premultiplied so edges stay clean.
#[must_use]
pub fn resize_rgba(pixels: &[u8], from: [u32; 2], to: [u32; 2]) -> Vec<u8> {
    let (w, h) = (from[0].max(1), from[1].max(1));
    let (nw, nh) = (to[0].max(1), to[1].max(1));
    let (sx, sy) = (w as f32 / nw as f32, h as f32 / nh as f32);
    let mut out = vec![0; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let pixel = if sx <= 1.0 && sy <= 1.0 {
                crate::warp::sample(pixels, w, h, (x as f32 + 0.5) * sx, (y as f32 + 0.5) * sy)
            } else {
                // The average of the source pixels this one covers.
                let (x0, x1) = (
                    (x as f32 * sx) as u32,
                    (((x + 1) as f32 * sx).ceil() as u32).min(w),
                );
                let (y0, y1) = (
                    (y as f32 * sy) as u32,
                    (((y + 1) as f32 * sy).ceil() as u32).min(h),
                );
                let (mut sum, mut alpha, mut count) = ([0.0_f32; 3], 0.0_f32, 0.0_f32);
                for py in y0..y1.max(y0 + 1) {
                    for px in x0..x1.max(x0 + 1) {
                        let at = ((py.min(h - 1) * w + px.min(w - 1)) * 4) as usize;
                        let a = f32::from(pixels[at + 3]) / 255.0;
                        for (channel, value) in sum.iter_mut().zip(&pixels[at..at + 3]) {
                            *channel += f32::from(*value) * a;
                        }
                        alpha += a;
                        count += 1.0;
                    }
                }
                if alpha <= 0.0 {
                    [0; 4]
                } else {
                    [
                        (sum[0] / alpha).round() as u8,
                        (sum[1] / alpha).round() as u8,
                        (sum[2] / alpha).round() as u8,
                        (alpha / count * 255.0).round() as u8,
                    ]
                }
            };
            out[((y * nw + x) * 4) as usize..][..4].copy_from_slice(&pixel);
        }
    }
    out
}

/// `tile` (straight RGBA8) made to fit inside `size` keeping its shape,
/// never larger than it is, in the middle of a clear picture of `size`.
#[must_use]
pub fn fitted(tile: &RasterTile2D, size: [u32; 2]) -> Option<RasterTile2D> {
    let from = [tile.width(), tile.height()];
    let scale = (f64::from(size[0]) / f64::from(from[0]))
        .min(f64::from(size[1]) / f64::from(from[1]))
        .min(1.0);
    let to = [
        ((f64::from(from[0]) * scale).round() as u32).max(1),
        ((f64::from(from[1]) * scale).round() as u32).max(1),
    ];
    let rgba = match tile.format() {
        PixelFormat::Rgba8 => tile.bytes()?.to_vec(),
        _ => return None,
    };
    let fitted = if to == from {
        rgba
    } else {
        resize_rgba(&rgba, from, to)
    };
    let mut out = vec![0_u8; (size[0] * size[1] * 4) as usize];
    let (left, top) = ((size[0] - to[0]) / 2, (size[1] - to[1]) / 2);
    for y in 0..to[1] {
        let row = (y * to[0] * 4) as usize;
        let at = (((top + y) * size[0] + left) * 4) as usize;
        out[at..at + (to[0] * 4) as usize]
            .copy_from_slice(&fitted[row..row + (to[0] * 4) as usize]);
    }
    RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, out).ok()
}

/// Text shapes (frame panels, balloons) with every number pair moved by
/// `shift` and then scaled by `scale`.
pub(super) fn move_shapes(text: &str, shift: [f32; 2], scale: [f32; 2]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut axis = 0;
    let mut number = String::new();
    let flush = |number: &mut String, out: &mut String, axis: &mut usize| {
        if number.is_empty() {
            return;
        }
        let value: f32 = number.parse().unwrap_or(0.0);
        let moved = (value + shift[*axis % 2]) * scale[*axis % 2];
        out.push_str(&moved.round().to_string());
        *axis += 1;
        number.clear();
    };
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == '.' || (ch == '-' && number.is_empty()) {
            number.push(ch);
        } else {
            flush(&mut number, &mut out, &mut axis);
            if ch == ';' || ch == ':' {
                axis = 0;
            }
            out.push(ch);
        }
    }
    flush(&mut number, &mut out, &mut axis);
    out
}

/// The edit that changes `picture`'s canvas to `size`, every layer's
/// pixels moved by `shift` (where the old top left corner lands) and cut
/// or widened to the new canvas: Change Canvas Size and Crop.
///
/// # Errors
/// If the size is out of range.
pub fn resize_canvas(
    arena: &Arena,
    picture: EntityId,
    size: [u32; 2],
    shift: [i64; 2],
) -> Result<Edit, String> {
    let [width, height] = size;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(tr!("canvas-must-1-pixels-side", MAX_SIDE = MAX_SIDE));
    }
    let mut edit = Edit::new(tr!("change-canvas-size"))
        .then(SetProperty::new(picture, picture::WIDTH, i64::from(width)))
        .then(SetProperty::new(
            picture,
            picture::HEIGHT,
            i64::from(height),
        ));
    for info in layers(arena, picture) {
        let Some(raster) = raster(arena, info.id) else {
            continue;
        };
        let (Some(bytes), PixelFormat::Rgba8) = (raster.bytes(), raster.format()) else {
            continue;
        };
        let at = [
            info.style.offset[0] + shift[0],
            info.style.offset[1] + shift[1],
        ];
        let mut pixels = vec![0; (width * height * 4) as usize];
        for y in 0..raster.height() {
            let ty = i64::from(y) + at[1];
            if ty < 0 || ty >= i64::from(height) {
                continue;
            }
            for x in 0..raster.width() {
                let tx = i64::from(x) + at[0];
                if tx < 0 || tx >= i64::from(width) {
                    continue;
                }
                let from = ((y * raster.width() + x) * 4) as usize;
                let to = ((ty as u32 * width + tx as u32) * 4) as usize;
                pixels[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
            }
        }
        let tile = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
            .map_err(|error| error.to_string())?;
        edit = edit.then(sue_core::txn::ReplaceComponent::new(
            info.id,
            tile,
            tr!("change-canvas-size"),
        ));
        if info.style.offset != [0, 0] {
            edit = edit
                .then(SetProperty::new(info.id, picture::X, 0_i64))
                .then(SetProperty::new(info.id, picture::Y, 0_i64));
        }
        edit = shapes_moved(
            arena,
            info.id,
            edit,
            [shift[0] as f32, shift[1] as f32],
            [1.0, 1.0],
        );
    }
    Ok(edit)
}

/// `edit` followed by moving a frame's or balloon layer's shapes too.
fn shapes_moved(
    arena: &Arena,
    layer: EntityId,
    mut edit: Edit,
    shift: [f32; 2],
    scale: [f32; 2],
) -> Edit {
    let Ok(entity) = arena.get_by_id(layer) else {
        return edit;
    };
    for key in [crate::manga::PANELS, crate::manga::SHAPES] {
        if let Some(text) = entity.property(key).and_then(Value::as_str) {
            edit = edit.then(SetProperty::new(
                layer,
                key,
                move_shapes(text, shift, scale),
            ));
        }
    }
    edit
}

/// The edit that resamples `picture` and every layer to `size`, at
/// `dpi`: Change Image Resolution.
///
/// # Errors
/// If the size is out of range.
pub fn resample(
    arena: &Arena,
    picture: EntityId,
    size: [u32; 2],
    dpi: f64,
) -> Result<Edit, String> {
    let [width, height] = size;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(tr!("picture-must-1-pixels-side", MAX_SIDE = MAX_SIDE));
    }
    let [old_w, old_h] = size_of(arena, picture)?;
    let scale = [width as f32 / old_w as f32, height as f32 / old_h as f32];
    let mut edit = Edit::new(tr!("change-image-resolution"))
        .then(SetProperty::new(picture, picture::WIDTH, i64::from(width)))
        .then(SetProperty::new(
            picture,
            picture::HEIGHT,
            i64::from(height),
        ))
        .then(SetProperty::new(picture, picture::DPI, dpi));
    for info in layers(arena, picture) {
        let Some(raster) = raster(arena, info.id) else {
            continue;
        };
        let (Some(bytes), PixelFormat::Rgba8) = (raster.bytes(), raster.format()) else {
            continue;
        };
        let to = [
            ((raster.width() as f32 * scale[0]).round() as u32).max(1),
            ((raster.height() as f32 * scale[1]).round() as u32).max(1),
        ];
        let pixels = resize_rgba(bytes, [raster.width(), raster.height()], to);
        let tile = RasterTile2D::from_bytes(to[0], to[1], PixelFormat::Rgba8, pixels)
            .map_err(|error| error.to_string())?;
        edit = edit.then(sue_core::txn::ReplaceComponent::new(
            info.id,
            tile,
            tr!("change-image-resolution"),
        ));
        if info.style.offset != [0, 0] {
            edit = edit
                .then(SetProperty::new(
                    info.id,
                    picture::X,
                    (info.style.offset[0] as f32 * scale[0]).round() as i64,
                ))
                .then(SetProperty::new(
                    info.id,
                    picture::Y,
                    (info.style.offset[1] as f32 * scale[1]).round() as i64,
                ));
        }
        edit = shapes_moved(arena, info.id, edit, [0.0, 0.0], scale);
    }
    Ok(edit)
}

/// The edit that turns vector `layer` into a raster layer in its place
/// (Rasterize): its pixels, name, properties and folder kept, its lines
/// let go. Returns the new layer too.
///
/// # Errors
/// If `layer` is not a vector layer with pixels.
pub fn rasterize(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
) -> Result<(Edit, EntityId), String> {
    if !crate::vector::is_vector(arena, layer) {
        return Err(tr!("only-vector-layers-rasterize"));
    }
    let entity = arena.get_by_id(layer).map_err(|error| error.to_string())?;
    let raster = raster(arena, layer)
        .ok_or(tr!("the-layer-has-no-pixels"))?
        .clone();
    let mut fresh = Entity::new(entity.name(), raster).with_class(class::LAYER);
    for (key, value) in entity.properties().iter() {
        fresh = fresh.with_property(key, value.clone());
    }
    let id = fresh.id();
    let order = key_above(arena, picture, Some(layer));
    let mut edit = Edit::new(tr!("rasterize")).then(
        SpawnEntity::new(fresh).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
    );
    if let Some(folder) = folder_of(arena, layer) {
        edit = edit.then(Relate::new(Relation::new(folder, MEMBER, id)));
    }
    Ok((edit.then(delete(layer)), id))
}

/// The edit that turns `picture` a quarter turn (Rotate 90 Degrees Right
/// when `clockwise`, else Left): the canvas's sides swap, and every
/// layer's pixels, mask and vector lines turn with it.
///
/// # Errors
/// If the picture has no size or a layer will not turn.
pub fn rotate_canvas(arena: &Arena, picture: EntityId, clockwise: bool) -> Result<Edit, String> {
    let [width, height] = size_of(arena, picture)?;
    let label = &if clockwise {
        tr!("rotate-90-degrees-right")
    } else {
        tr!("rotate-90-degrees-left")
    };
    let mut edit = Edit::new(label)
        .then(SetProperty::new(picture, picture::WIDTH, i64::from(height)))
        .then(SetProperty::new(picture, picture::HEIGHT, i64::from(width)));
    let turn = |id: EntityId, offset: [i64; 2], edit: Edit| -> Result<Edit, String> {
        let Some(raster) = raster(arena, id) else {
            return Ok(edit);
        };
        let (Some(bytes), PixelFormat::Rgba8) = (raster.bytes(), raster.format()) else {
            return Ok(edit);
        };
        let (w, h) = (raster.width(), raster.height());
        let mut out = vec![0_u8; bytes.len()];
        for ny in 0..w {
            for nx in 0..h {
                let (ox, oy) = if clockwise {
                    (ny, h - 1 - nx)
                } else {
                    (w - 1 - ny, nx)
                };
                let from = ((oy * w + ox) * 4) as usize;
                let to = ((ny * h + nx) * 4) as usize;
                out[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
            }
        }
        let tile = RasterTile2D::from_bytes(h, w, PixelFormat::Rgba8, out)
            .map_err(|error| error.to_string())?;
        let [x, y] = offset;
        let moved = if clockwise {
            [i64::from(height) - (y + i64::from(h)), x]
        } else {
            [y, i64::from(width) - (x + i64::from(w))]
        };
        let mut edit = edit.then(sue_core::txn::ReplaceComponent::new(id, tile, label));
        if moved != [0, 0] || offset != [0, 0] {
            edit = edit
                .then(SetProperty::new(id, picture::X, moved[0]))
                .then(SetProperty::new(id, picture::Y, moved[1]));
        }
        Ok(edit)
    };
    for info in layers(arena, picture) {
        edit = turn(info.id, info.style.offset, edit)?;
        if let Some(mask) = mask_of(arena, info.id) {
            edit = turn(mask, info.style.offset, edit)?;
        }
        if crate::vector::is_vector(arena, info.id) {
            let mut lines = crate::vector::lines(&crate::vector::geometry(arena, info.id));
            for line in &mut lines {
                for sample in &mut line.samples {
                    let [x, y] = sample.at;
                    sample.at = if clockwise {
                        [height as f32 - y, x]
                    } else {
                        [y, width as f32 - x]
                    };
                }
            }
            edit = edit.then(sue_core::txn::SetExtension::set(
                info.id,
                crate::vector::to_geometry(&lines),
            ));
        }
    }
    Ok(edit)
}

/// The edit that turns raster `layer` into a vector layer in its place
/// (Convert Layer): its pixels traced to center lines, its name,
/// properties and folder kept. Returns the new layer.
///
/// # Errors
/// If `layer` is not a raster layer with something on it.
pub fn vectorize(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
) -> Result<(Edit, EntityId), String> {
    if crate::vector::is_vector(arena, layer) {
        return Err(tr!("layer-vector-layer-already"));
    }
    let entity = arena.get_by_id(layer).map_err(|error| error.to_string())?;
    let source = raster(arena, layer).ok_or(tr!("only-raster-layers-convert-vector"))?;
    let bytes = source
        .bytes()
        .ok_or(tr!("only-rgba-layers-convert-vector"))?;
    let [ox, oy] = LayerStyle::of(entity).offset;
    let mut lines = crate::vector::trace(bytes, source.width(), source.height());
    if lines.is_empty() {
        return Err(tr!("there-nothing-layer-trace"));
    }
    for line in &mut lines {
        for sample in &mut line.samples {
            sample.at[0] += ox as f32;
            sample.at[1] += oy as f32;
        }
    }
    let [width, height] = size_of(arena, picture)?;
    let drawn =
        crate::vector::render(&lines, width, height).ok_or(tr!("the-lines-cannot-be-drawn"))?;
    let mut fresh = Entity::new(entity.name(), drawn)
        .with_class(crate::vector::CLASS)
        .with_ext(crate::vector::to_geometry(&lines));
    for (key, value) in entity.properties().iter() {
        if key != picture::X && key != picture::Y {
            fresh = fresh.with_property(key, value.clone());
        }
    }
    let id = fresh.id();
    let order = key_above(arena, picture, Some(layer));
    let mut edit = Edit::new(tr!("convert-layer")).then(
        SpawnEntity::new(fresh).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
    );
    if let Some(folder) = folder_of(arena, layer) {
        edit = edit.then(Relate::new(Relation::new(folder, MEMBER, id)));
    }
    Ok((edit.then(delete(layer)), id))
}
