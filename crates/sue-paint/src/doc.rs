//! Pictures in the arena and the undoable edits Sue Paint makes to them.
//!
//! Every picture is the shared layered-picture shape of
//! [`sue_model::picture`]: an entity of class `sue.image`, its layers as
//! ordered children. Each function here builds a transaction from the
//! backbone's generic edits ([`SpawnEntity`], [`DespawnEntity`],
//! [`SetProperty`], [`Relate`], [`RasterEdit`]) by entity id; the studio
//! applies it through the global undo history.

use sue_core::txn::{
    Batch, DespawnEntity, RasterEdit, RasterPatch, Relate, SetProperty, SpawnEntity, Unrelate,
};
use sue_core::{
    Arena, CHILD, Color, ComponentData, ComponentKind, Entity, EntityId, OrderKey, PixelFormat,
    RasterTile2D, Relation, Value,
};
use sue_model::class;
use sue_model::picture::{self, LayerKind, LayerStyle};

use crate::composite::{RenderOptions, render};

/// An edit of a picture: several arena steps undone as one.
pub type Edit = Batch<Arena>;

/// The largest side of a new picture, in pixels.
pub const MAX_SIDE: u32 = 10_000;

/// What a new picture looks like.
#[derive(Clone, Debug, PartialEq)]
pub struct NewPicture {
    /// Its name.
    pub name: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Print resolution in pixels per inch.
    pub dpi: f64,
    /// The paper color, or `None` for a clear background.
    pub paper: Option<Color>,
    /// It records its timelapse from the start.
    pub timelapse: bool,
}

impl Default for NewPicture {
    fn default() -> Self {
        Self {
            name: "Illustration".to_owned(),
            width: 1600,
            height: 1200,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
            timelapse: false,
        }
    }
}

/// The edit that creates a picture with its paper and one empty layer, and
/// the picture's id.
///
/// # Errors
/// If the size is zero or larger than [`MAX_SIDE`].
pub fn create(spec: &NewPicture) -> Result<(Edit, EntityId), String> {
    if spec.width == 0 || spec.height == 0 || spec.width > MAX_SIDE || spec.height > MAX_SIDE {
        return Err(format!(
            "A picture must be 1 to {MAX_SIDE} pixels a side, not {} by {}",
            spec.width, spec.height
        ));
    }
    let image = Entity::empty(spec.name.clone(), class::IMAGE)
        .with_property(picture::WIDTH, i64::from(spec.width))
        .with_property(picture::HEIGHT, i64::from(spec.height))
        .with_property(picture::DPI, spec.dpi)
        .with_property(crate::timelapse::RECORD, spec.timelapse);
    let id = image.id();
    let mut edit = Edit::new(format!("New {}", spec.name)).then(SpawnEntity::new(image));
    let mut keys = OrderKey::sequence(None, 2).into_iter();
    if let Some(color) = spec.paper {
        let paper = Entity::empty("Paper", class::PAPER).with_property(picture::COLOR, color);
        let order = keys.next().unwrap_or_default();
        let paper_id = paper.id();
        edit = edit.then(
            SpawnEntity::new(paper)
                .with_relation(Relation::new(id, CHILD, paper_id).ordered(order)),
        );
    }
    let raster = RasterTile2D::new(spec.width, spec.height, PixelFormat::Rgba8)
        .map_err(|error| error.to_string())?;
    let layer = Entity::new("Layer 1", raster).with_class(class::LAYER);
    let layer_id = layer.id();
    let order = keys.next().unwrap_or_default();
    edit = edit.then(
        SpawnEntity::new(layer).with_relation(Relation::new(id, CHILD, layer_id).ordered(order)),
    );
    Ok((edit, id))
}

/// Makes a picture of `layer`'s size whose one layer is `layer` itself (a
/// photo on the canvas): the same entity, never a copy, so painting on it
/// changes the photo, and undo takes it back.
///
/// # Errors
/// If `layer` has no 8-bit color pixels, or already is a layer.
pub fn adopt(arena: &Arena, layer: EntityId, name: &str) -> Result<(Edit, EntityId), String> {
    let raster = raster(arena, layer).ok_or("this has no pixels to paint on")?;
    if raster.format() != PixelFormat::Rgba8 {
        return Err("only 8-bit color pictures can be painted on; convert it first".to_owned());
    }
    if arena.relations().parent(layer, CHILD).is_some() {
        return Err("this is already a layer of a picture".to_owned());
    }
    let (width, height) = (raster.width(), raster.height());
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!(
            "A picture must be at most {MAX_SIDE} pixels a side"
        ));
    }
    let image = Entity::empty(name, class::IMAGE)
        .with_property(picture::WIDTH, i64::from(width))
        .with_property(picture::HEIGHT, i64::from(height))
        .with_property(picture::DPI, 72.0);
    let id = image.id();
    let edit = Edit::new(format!("Open {name} in Sue Paint"))
        .then(SpawnEntity::new(image))
        .then(Relate::new(
            Relation::new(id, CHILD, layer).ordered(OrderKey::default()),
        ));
    Ok((edit, id))
}

/// One layer as the panels show it.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerInfo {
    /// The layer entity.
    pub id: EntityId,
    /// Its name.
    pub name: String,
    /// What it is.
    pub kind: LayerKind,
    /// How it is laid over the layers below.
    pub style: LayerStyle,
    /// Locked against edits.
    pub locked: bool,
    /// Painting keeps its alpha.
    pub alpha_locked: bool,
    /// Its arena revision.
    pub revision: u64,
    /// Its place among its siblings.
    pub order: OrderKey,
}

/// The layers of `picture`, bottom first.
#[must_use]
pub fn layers(arena: &Arena, picture: EntityId) -> Vec<LayerInfo> {
    picture::layers(arena, picture)
        .into_iter()
        .filter_map(|id| {
            let entity = arena.get_by_id(id).ok()?;
            Some(LayerInfo {
                id,
                name: entity.name().to_owned(),
                kind: LayerKind::of(entity),
                style: LayerStyle::of(entity),
                locked: picture::is_locked(entity),
                alpha_locked: picture::is_alpha_locked(entity),
                revision: entity.revision(),
                order: arena.relations().get(picture, CHILD, id)?.order,
            })
        })
        .collect()
}

// ----- Canvas size and resolution -------------------------------------------------

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

/// Text shapes (frame panels, balloons) with every number pair moved by
/// `shift` and then scaled by `scale`.
fn move_shapes(text: &str, shift: [f32; 2], scale: [f32; 2]) -> String {
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
        return Err(format!("A canvas must be 1 to {MAX_SIDE} pixels a side"));
    }
    let mut edit = Edit::new("Change Canvas Size")
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
            "Change Canvas Size",
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
        return Err(format!("A picture must be 1 to {MAX_SIDE} pixels a side"));
    }
    let [old_w, old_h] = size_of(arena, picture)?;
    let scale = [width as f32 / old_w as f32, height as f32 / old_h as f32];
    let mut edit = Edit::new("Change Image Resolution")
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
            "Change Image Resolution",
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

// ----- Correction layers -------------------------------------------------------------

/// The class of a correction layer: it changes the picture below it.
pub const CORRECTION: &str = "sue.paint.correction";
/// A correction layer's correction (`Text`, `Filter::to_text`).
pub const CORRECTION_FILTER: &str = "correction.filter";

/// The edit that puts a correction layer named `name` doing `text` right
/// above `layer` (or on top), and its id.
#[must_use]
pub fn new_correction(
    arena: &Arena,
    picture: EntityId,
    layer: Option<EntityId>,
    name: &str,
    text: &str,
) -> (Edit, EntityId) {
    let correction = Entity::empty(name, CORRECTION).with_property(CORRECTION_FILTER, text);
    let id = correction.id();
    let order = key_above(arena, picture, layer);
    let edit = Edit::new(format!("New {name}")).then(
        SpawnEntity::new(correction)
            .with_relation(Relation::new(picture, CHILD, id).ordered(order)),
    );
    (edit, id)
}

// ----- Layer masks -------------------------------------------------------------------

/// The relation from a layer to its mask.
pub const MASK: &str = "sue.paint.mask";
/// A layer's mask is in use (`Bool`, on the layer).
pub const MASK_ON: &str = "layer.mask_on";
/// The class of a layer mask: an RGBA raster whose alpha is how much of
/// its layer shows.
pub const MASK_CLASS: &str = "sue.paint.layer_mask";

/// The mask of `layer`, if it has one.
#[must_use]
pub fn mask_of(arena: &Arena, layer: EntityId) -> Option<EntityId> {
    arena.relations().targets(layer, MASK).into_iter().next()
}

/// `true` if `layer`'s mask is in use.
#[must_use]
pub fn mask_on(arena: &Arena, layer: EntityId) -> bool {
    arena
        .get_by_id(layer)
        .ok()
        .and_then(|entity| entity.property(MASK_ON))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// The edit that gives `layer` a mask showing it where `show` (picture
/// pixels, 0 to 1) says, and the mask's id: Mask Outside Selection is the
/// selection, Mask Selection its opposite.
///
/// # Errors
/// If the layer has no pixels or already has a mask.
pub fn new_mask(
    arena: &Arena,
    layer: EntityId,
    show: impl Fn(i64, i64) -> f32,
) -> Result<(Edit, EntityId), String> {
    if mask_of(arena, layer).is_some() {
        return Err("This layer has a mask already".to_owned());
    }
    let tile = raster(arena, layer).ok_or("Only a raster layer takes a mask")?;
    let (width, height) = (tile.width(), tile.height());
    let offset = arena
        .get_by_id(layer)
        .map(LayerStyle::of)
        .map(|style| style.offset)
        .unwrap_or_default();
    let mut pixels = vec![255; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let cover = show(i64::from(x) + offset[0], i64::from(y) + offset[1]).clamp(0.0, 1.0);
            pixels[((y * width + x) * 4 + 3) as usize] = (cover * 255.0).round() as u8;
        }
    }
    let mask = Entity::new(
        "Mask",
        RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
            .map_err(|error| error.to_string())?,
    )
    .with_class(MASK_CLASS);
    let id = mask.id();
    let edit = Edit::new("New Layer Mask")
        .then(SpawnEntity::new(mask).with_relation(Relation::new(layer, MASK, id)))
        .then(SetProperty::new(layer, MASK_ON, true));
    Ok((edit, id))
}

/// The edit that takes `layer`'s mask away (undo brings it back).
#[must_use]
pub fn delete_mask(arena: &Arena, layer: EntityId) -> Option<Edit> {
    let mask = mask_of(arena, layer)?;
    Some(Edit::new("Delete Mask").then(DespawnEntity::new(mask)))
}

/// The edit that applies `layer`'s mask to its pixels, then deletes it.
///
/// # Errors
/// If the layer or its mask has no pixels, or they differ in size.
pub fn apply_mask(arena: &Arena, layer: EntityId) -> Result<Edit, String> {
    let mask = mask_of(arena, layer).ok_or("This layer has no mask")?;
    let (tile, shape) = (
        raster(arena, layer).ok_or("The layer has no pixels")?,
        raster(arena, mask).ok_or("The mask has no pixels")?,
    );
    if [tile.width(), tile.height()] != [shape.width(), shape.height()] {
        return Err("The mask and its layer differ in size".to_owned());
    }
    let mut pixels = tile.bytes().ok_or("The layer has no pixels")?.to_vec();
    let alpha = shape.bytes().ok_or("The mask has no pixels")?;
    for (pixel, cover) in pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(alpha.as_chunks::<4>().0)
    {
        pixel[3] = ((u16::from(pixel[3]) * u16::from(cover[3]) + 127) / 255) as u8;
    }
    let applied = RasterTile2D::from_bytes(tile.width(), tile.height(), PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())?;
    Ok(Edit::new("Apply Mask to Layer")
        .then(sue_core::txn::ReplaceComponent::new(
            layer,
            applied,
            "Apply Mask to Layer",
        ))
        .then(DespawnEntity::new(mask)))
}

// ----- Layer folders ---------------------------------------------------------------

/// The relation from a layer folder to each layer in it.
pub const MEMBER: &str = "sue.paint.member";
/// A folder shows its layers in the palette (`Bool`).
pub const OPEN: &str = "layer.open";

/// `true` for a layer folder: a `sue.folder` in a picture's stack.
#[must_use]
pub fn is_folder(arena: &Arena, id: EntityId) -> bool {
    arena
        .get_by_id(id)
        .is_ok_and(|entity| entity.class() == class::FOLDER)
}

/// The folder `layer` is in.
#[must_use]
pub fn folder_of(arena: &Arena, layer: EntityId) -> Option<EntityId> {
    arena.relations().sources(layer, MEMBER).into_iter().next()
}

/// The folders around `layer`, innermost first.
#[must_use]
pub fn folders_around(arena: &Arena, layer: EntityId) -> Vec<EntityId> {
    let mut out = Vec::new();
    let mut at = folder_of(arena, layer);
    while let Some(folder) = at {
        if out.contains(&folder) || out.len() > 32 {
            break;
        }
        out.push(folder);
        at = folder_of(arena, folder);
    }
    out
}

/// The layers in `folder` (and in the folders in it), in stacking order.
#[must_use]
pub fn members(arena: &Arena, picture: EntityId, folder: EntityId) -> Vec<EntityId> {
    layers(arena, picture)
        .into_iter()
        .map(|info| info.id)
        .filter(|id| folders_around(arena, *id).contains(&folder))
        .collect()
}

/// `true` if the folder shows its layers in the palette.
#[must_use]
pub fn is_open(arena: &Arena, folder: EntityId) -> bool {
    arena
        .get_by_id(folder)
        .ok()
        .and_then(|entity| entity.property(OPEN))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// The edit that makes a folder right above `layer` (or on top), with
/// `layer` in it when `insert` is set, and the folder's id.
#[must_use]
pub fn new_folder(
    arena: &Arena,
    picture: EntityId,
    layer: Option<EntityId>,
    insert: bool,
) -> (Edit, EntityId) {
    let count = layers(arena, picture)
        .iter()
        .filter(|info| is_folder(arena, info.id))
        .count();
    let folder =
        Entity::empty(format!("Folder {}", count + 1), class::FOLDER).with_property(OPEN, true);
    let id = folder.id();
    let order = key_above(arena, picture, layer);
    let mut edit = Edit::new(if insert {
        "Create Folder and Insert Layer"
    } else {
        "New Layer Folder"
    })
    .then(SpawnEntity::new(folder).with_relation(Relation::new(picture, CHILD, id).ordered(order)));
    if let Some(layer) = layer.filter(|_| insert) {
        if let Some(outer) = folder_of(arena, layer) {
            // The new folder takes the layer's place in its folder.
            edit = edit
                .then(Unrelate::new(outer, MEMBER, layer))
                .then(Relate::new(Relation::new(outer, MEMBER, id)));
        }
        edit = edit.then(Relate::new(Relation::new(id, MEMBER, layer)));
    }
    (edit, id)
}

/// The edit that puts `layer` in `folder` (`None`: in no folder).
#[must_use]
pub fn set_folder(arena: &Arena, layer: EntityId, folder: Option<EntityId>) -> Option<Edit> {
    let now = folder_of(arena, layer);
    if now == folder || folder == Some(layer) {
        return None;
    }
    let mut edit = Edit::new(if folder.is_some() {
        "Into Folder"
    } else {
        "Out of Folder"
    });
    if let Some(now) = now {
        edit = edit.then(Unrelate::new(now, MEMBER, layer));
    }
    if let Some(folder) = folder {
        edit = edit.then(Relate::new(Relation::new(folder, MEMBER, layer)));
    }
    Some(edit)
}

/// The edit that removes `folder` and every layer in it.
#[must_use]
pub fn delete_folder(arena: &Arena, picture: EntityId, folder: EntityId) -> Edit {
    let mut edit = Edit::new("Delete Folder");
    for layer in members(arena, picture, folder) {
        edit = edit.then(DespawnEntity::new(layer));
    }
    edit.then(DespawnEntity::new(folder))
}

/// The pixels of a raster layer.
#[must_use]
pub fn raster(arena: &Arena, layer: EntityId) -> Option<&RasterTile2D> {
    arena
        .get_by_id(layer)
        .ok()?
        .component(ComponentKind::RasterTile2D)?
        .as_raster()
}

/// The pixels of a raster layer, to paint in place (this marks it changed).
pub fn raster_mut(arena: &mut Arena, layer: EntityId) -> Option<&mut RasterTile2D> {
    match arena
        .get_by_id_mut(layer)
        .ok()?
        .component_mut(ComponentKind::RasterTile2D)?
    {
        ComponentData::RasterTile2D(raster) => Some(raster),
        _ => None,
    }
}

/// A name like `Layer 3` that no layer of `picture` has yet.
#[must_use]
pub fn new_layer_name(arena: &Arena, picture: EntityId) -> String {
    let highest = layers(arena, picture)
        .iter()
        .filter_map(|layer| layer.name.strip_prefix("Layer ")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("Layer {}", highest + 1)
}

/// The order key for a new layer right above `below` (or at the top).
fn key_above(arena: &Arena, picture: EntityId, below: Option<EntityId>) -> OrderKey {
    let stack = layers(arena, picture);
    let Some(index) = below.and_then(|id| stack.iter().position(|layer| layer.id == id)) else {
        return arena.relations().next_order(picture, CHILD);
    };
    let low = &stack[index].order;
    match stack.get(index + 1) {
        Some(next) if next.order > *low => OrderKey::between(Some(low), Some(&next.order)),
        _ => OrderKey::between(Some(low), None),
    }
}

/// The size of a picture.
fn size_of(arena: &Arena, picture: EntityId) -> Result<[u32; 2], String> {
    arena
        .get_by_id(picture)
        .ok()
        .and_then(picture::size)
        .ok_or_else(|| "The picture has no size".to_owned())
}

/// A new raster layer right above `below` (or at the top), clear or filled
/// with `fill` (straight RGBA).
///
/// # Errors
/// If the picture has no size.
pub fn add_raster_layer(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    name: String,
    fill: Option<[u8; 4]>,
) -> Result<(SpawnEntity, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    let raster = match fill {
        Some(rgba) => RasterTile2D::from_bytes(
            width,
            height,
            PixelFormat::Rgba8,
            rgba.repeat(width as usize * height as usize),
        ),
        None => RasterTile2D::new(width, height, PixelFormat::Rgba8),
    }
    .map_err(|error| error.to_string())?;
    let layer = Entity::new(name, raster).with_class(class::LAYER);
    let id = layer.id();
    let order = key_above(arena, picture, below);
    Ok((
        SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    ))
}

/// A new raster layer named `name` right above `below` (or at the top),
/// holding `raster` (the picture's size).
///
/// # Errors
/// If `raster` is not the picture's size.
pub fn add_layer_pixels(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    name: String,
    raster: RasterTile2D,
) -> Result<(SpawnEntity, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    if raster.width() != width || raster.height() != height {
        return Err("the layer is not the picture's size".to_owned());
    }
    let layer = Entity::new(name, raster).with_class(class::LAYER);
    let id = layer.id();
    let order = key_above(arena, picture, below);
    Ok((
        SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    ))
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
        return Err("Only vector layers rasterize".to_owned());
    }
    let entity = arena.get_by_id(layer).map_err(|error| error.to_string())?;
    let raster = raster(arena, layer)
        .ok_or("The layer has no pixels")?
        .clone();
    let mut fresh = Entity::new(entity.name(), raster).with_class(class::LAYER);
    for (key, value) in entity.properties().iter() {
        fresh = fresh.with_property(key, value.clone());
    }
    let id = fresh.id();
    let order = key_above(arena, picture, Some(layer));
    let mut edit = Edit::new("Rasterize").then(
        SpawnEntity::new(fresh).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
    );
    if let Some(folder) = folder_of(arena, layer) {
        edit = edit.then(Relate::new(Relation::new(folder, MEMBER, id)));
    }
    Ok((edit.then(delete(layer)), id))
}

/// New raster layers right above `below` (or at the top), in the order
/// given (the first lowest), each named and holding its raster (the
/// picture's size); their spawns and ids.
///
/// # Errors
/// If a raster is not the picture's size.
pub fn add_layers_pixels(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    layers_in: Vec<(String, RasterTile2D)>,
) -> Result<(Vec<SpawnEntity>, Vec<EntityId>), String> {
    let [width, height] = size_of(arena, picture)?;
    let stack = layers(arena, picture);
    let at = below.and_then(|id| stack.iter().position(|layer| layer.id == id));
    let mut low = match at {
        Some(index) => Some(stack[index].order.clone()),
        None => stack.last().map(|layer| layer.order.clone()),
    };
    let high = at
        .and_then(|index| stack.get(index + 1))
        .map(|layer| layer.order.clone());
    let (mut spawns, mut ids) = (Vec::new(), Vec::new());
    for (name, raster) in layers_in {
        if raster.width() != width || raster.height() != height {
            return Err("the layer is not the picture's size".to_owned());
        }
        let order = match (&low, &high) {
            (Some(low), Some(high)) if high > low => OrderKey::between(Some(low), Some(high)),
            (low, _) => OrderKey::between(low.as_ref(), None),
        };
        let layer = Entity::new(name, raster).with_class(class::LAYER);
        let id = layer.id();
        spawns.push(
            SpawnEntity::new(layer)
                .with_relation(Relation::new(picture, CHILD, id).ordered(order.clone())),
        );
        ids.push(id);
        low = Some(order);
    }
    Ok((spawns, ids))
}

/// A layer the fill and auto select can refer to (Set as Reference Layer,
/// `Bool`).
pub const REFERENCE: &str = "layer.reference";

/// `true` if `layer` is a reference layer.
#[must_use]
pub fn is_reference(arena: &Arena, layer: EntityId) -> bool {
    arena
        .get_by_id(layer)
        .ok()
        .and_then(|entity| entity.property(REFERENCE))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// A new vector layer right above `below` (or at the top): clear pixels
/// and no lines yet.
///
/// # Errors
/// If the picture has no size.
pub fn add_vector_layer(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    name: String,
) -> Result<(SpawnEntity, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    let raster =
        RasterTile2D::new(width, height, PixelFormat::Rgba8).map_err(|error| error.to_string())?;
    let layer = Entity::new(name, raster)
        .with_class(crate::vector::CLASS)
        .with_ext(sue_model::Geometry::new());
    let id = layer.id();
    let order = key_above(arena, picture, below);
    Ok((
        SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    ))
}

/// The edit that turns `picture` a quarter turn (Rotate 90 Degrees Right
/// when `clockwise`, else Left): the canvas's sides swap, and every
/// layer's pixels, mask and vector lines turn with it.
///
/// # Errors
/// If the picture has no size or a layer will not turn.
pub fn rotate_canvas(arena: &Arena, picture: EntityId, clockwise: bool) -> Result<Edit, String> {
    let [width, height] = size_of(arena, picture)?;
    let label = if clockwise {
        "Rotate 90 Degrees Right"
    } else {
        "Rotate 90 Degrees Left"
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

/// The edit that merges raster `ids` of `picture` into the lowest of them
/// (Merge Selected Layers): each laid over the one below as its blend
/// mode and opacity say, hidden ones left out. Returns the merged layer.
///
/// # Errors
/// If fewer than two of them are raster layers, or one is locked.
pub fn merge_layers(
    arena: &Arena,
    picture: EntityId,
    ids: &[EntityId],
) -> Result<(Edit, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    let chosen: Vec<LayerInfo> = layers(arena, picture)
        .into_iter()
        .filter(|info| {
            ids.contains(&info.id) && info.kind == LayerKind::Raster && !is_folder(arena, info.id)
        })
        .collect();
    if chosen.len() < 2 {
        return Err(
            "Pick two or more raster layers to merge (Ctrl+click in the Layer palette)".to_owned(),
        );
    }
    if let Some(locked) = chosen.iter().find(|info| info.locked) {
        return Err(format!("{} is locked", locked.name));
    }
    let mut color = vec![[0.0_f32; 4]; (width * height) as usize];
    for info in chosen.iter().filter(|info| info.style.visible) {
        let Some(raster) = raster(arena, info.id) else {
            continue;
        };
        let (Some(bytes), PixelFormat::Rgba8) = (raster.bytes(), raster.format()) else {
            continue;
        };
        let [ox, oy] = info.style.offset;
        for y in 0..raster.height() {
            let ty = i64::from(y) + oy;
            if ty < 0 || ty >= i64::from(height) {
                continue;
            }
            for x in 0..raster.width() {
                let tx = i64::from(x) + ox;
                if tx < 0 || tx >= i64::from(width) {
                    continue;
                }
                let mut source = unit(&bytes[((y * raster.width() + x) * 4) as usize..][..4]);
                source[3] *= info.style.opacity;
                let at = (ty as u32 * width + tx as u32) as usize;
                color[at] = info.style.blend.composite(color[at], source);
            }
        }
    }
    let pixels: Vec<u8> = color.iter().flat_map(|pixel| self::bytes(*pixel)).collect();
    let tile = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())?;
    let into = chosen[0].id;
    let mut edit = Edit::new("Merge Selected Layers")
        .then(sue_core::txn::ReplaceComponent::new(
            into,
            tile,
            "Merge Selected Layers",
        ))
        .then(SetProperty::new(into, picture::X, 0_i64))
        .then(SetProperty::new(into, picture::Y, 0_i64))
        .then(SetProperty::new(into, picture::OPACITY, 1.0))
        .then(SetProperty::new(
            into,
            picture::BLEND,
            sue_model::BlendMode::Normal.name(),
        ))
        .then(SetProperty::new(into, picture::VISIBLE, true));
    for info in &chosen[1..] {
        edit = edit.then(delete(info.id));
    }
    Ok((edit, into))
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
        return Err("The layer is a vector layer already".to_owned());
    }
    let entity = arena.get_by_id(layer).map_err(|error| error.to_string())?;
    let source = raster(arena, layer).ok_or("Only raster layers convert to vector")?;
    let bytes = source.bytes().ok_or("Only RGBA layers convert to vector")?;
    let [ox, oy] = LayerStyle::of(entity).offset;
    let mut lines = crate::vector::trace(bytes, source.width(), source.height());
    if lines.is_empty() {
        return Err("There is nothing on the layer to trace".to_owned());
    }
    for line in &mut lines {
        for sample in &mut line.samples {
            sample.at[0] += ox as f32;
            sample.at[1] += oy as f32;
        }
    }
    let [width, height] = size_of(arena, picture)?;
    let drawn = crate::vector::render(&lines, width, height).ok_or("The lines cannot be drawn")?;
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
    let mut edit = Edit::new("Convert Layer").then(
        SpawnEntity::new(fresh).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
    );
    if let Some(folder) = folder_of(arena, layer) {
        edit = edit.then(Relate::new(Relation::new(folder, MEMBER, id)));
    }
    Ok((edit.then(delete(layer)), id))
}

/// A new vector layer right above `below` (or at the top) holding
/// `lines`, drawn (File, Import, Vector).
///
/// # Errors
/// If the picture has no size.
pub fn add_vector_lines(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    name: String,
    lines: &[crate::vector::Line],
) -> Result<(SpawnEntity, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    let raster = crate::vector::render(lines, width, height).ok_or("The lines cannot be drawn")?;
    let layer = Entity::new(name, raster)
        .with_class(crate::vector::CLASS)
        .with_ext(crate::vector::to_geometry(lines));
    let id = layer.id();
    let order = key_above(arena, picture, below);
    Ok((
        SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    ))
}

/// A paper layer under every other layer.
#[must_use]
pub fn add_paper(arena: &Arena, picture: EntityId, color: Color) -> (SpawnEntity, EntityId) {
    let first = layers(arena, picture)
        .first()
        .map(|layer| layer.order.clone());
    let order = OrderKey::between(None, first.as_ref());
    let paper = Entity::empty("Paper", class::PAPER).with_property(picture::COLOR, color);
    let id = paper.id();
    (
        SpawnEntity::new(paper).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    )
}

/// A copy of `layer` right above it, under a new id.
///
/// # Errors
/// If the layer is gone.
pub fn duplicate(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
) -> Result<(SpawnEntity, EntityId), String> {
    let source = arena.get_by_id(layer).map_err(|error| error.to_string())?;
    let mut copy = source.clone().with_id(EntityId::new());
    copy.set_name(format!("{} copy", source.name()));
    let id = copy.id();
    let order = key_above(arena, picture, Some(layer));
    Ok((
        SpawnEntity::new(copy).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        id,
    ))
}

/// Removes a layer (undo brings it back in its place).
#[must_use]
pub fn delete(layer: EntityId) -> DespawnEntity {
    DespawnEntity::new(layer)
}

/// Moves `layer` to position `index` of the stack (0 is the bottom).
#[must_use]
pub fn reorder(arena: &Arena, picture: EntityId, layer: EntityId, index: usize) -> Option<Edit> {
    let stack: Vec<LayerInfo> = layers(arena, picture);
    let from = stack.iter().position(|info| info.id == layer)?;
    let others: Vec<&LayerInfo> = stack.iter().filter(|info| info.id != layer).collect();
    let index = index.min(others.len());
    if index == from {
        return None;
    }
    let low = index
        .checked_sub(1)
        .and_then(|at| others.get(at))
        .map(|info| &info.order);
    let high = others.get(index).map(|info| &info.order);
    let order = match (low, high) {
        (Some(low), Some(high)) if low >= high => return None,
        _ => OrderKey::between(low, high),
    };
    Some(Edit::new("Move Layer").then(Relate::new(
        Relation::new(picture, CHILD, layer).ordered(order),
    )))
}

/// Sets one property of a layer or picture, as an undo step named for what
/// it changes.
pub fn set(id: EntityId, key: &str, value: impl Into<Value>) -> Edit {
    let value = value.into();
    Edit::new(label(key, &value)).then(SetProperty::new(id, key, value))
}

/// What people read in the history for setting `key` to `value`.
#[must_use]
pub fn label(key: &str, value: &Value) -> String {
    let on = value.as_bool().unwrap_or(true);
    let pick = |yes: &str, no: &str| if on { yes } else { no }.to_owned();
    match key {
        picture::VISIBLE => pick("Show Layer", "Hide Layer"),
        picture::LOCKED => pick("Lock Layer", "Unlock Layer"),
        picture::ALPHA_LOCKED => pick("Lock Transparent Pixels", "Unlock Transparent Pixels"),
        picture::CLIP => pick("Clip to Layer Below", "Release Clipping"),
        picture::DRAFT => pick("Set as Draft Layer", "Unset Draft Layer"),
        picture::OPACITY => "Layer Opacity".to_owned(),
        picture::BLEND => "Blend Mode".to_owned(),
        picture::COLOR => "Paper Color".to_owned(),
        picture::X | picture::Y => "Move Layer".to_owned(),
        crate::effects::BORDER => pick("Border Effect", "No Border Effect"),
        crate::effects::BORDER_WIDTH | crate::effects::BORDER_COLOR => "Border Effect".to_owned(),
        crate::effects::TONE => pick("Tone", "No Tone"),
        crate::effects::TONE_LINES | crate::effects::TONE_ANGLE => "Tone".to_owned(),
        crate::effects::COLOR => pick("Layer Color", "No Layer Color"),
        crate::effects::COLOR_VALUE => "Layer Color".to_owned(),
        crate::effects::EXPRESSION | crate::effects::THRESHOLD => "Expression Color".to_owned(),
        REFERENCE => pick("Set as Reference Layer", "Unset Reference Layer"),
        other => format!("Set {other}"),
    }
}

/// Merges `layer` into the raster layer below it, with its blend mode and
/// opacity, and removes it. Returns the edit and the layer merged into.
///
/// # Errors
/// If there is no raster layer right below, or either is not 8-bit RGBA.
pub fn merge_down(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
) -> Result<(Edit, EntityId), String> {
    let stack = layers(arena, picture);
    let index = stack
        .iter()
        .position(|info| info.id == layer)
        .ok_or("The layer is not in this picture")?;
    let below = index
        .checked_sub(1)
        .and_then(|at| stack.get(at))
        .filter(|info| info.kind == LayerKind::Raster)
        .ok_or("There is no raster layer below to merge into")?;
    if below.locked {
        return Err(format!("{} is locked", below.name));
    }
    let upper = raster(arena, layer).ok_or("Only raster layers merge")?;
    let lower = raster(arena, below.id).ok_or("Only raster layers merge")?;
    if upper.format() != PixelFormat::Rgba8 || lower.format() != PixelFormat::Rgba8 {
        return Err("Only RGBA layers merge".to_owned());
    }
    let style = stack[index].style;
    let mut edit = Edit::new(format!("Merge {}", stack[index].name));
    if style.visible
        && let Some(patch) = merged_patch(upper, style, lower, below.style.offset)
    {
        edit = edit.then(RasterEdit::new(below.id, "Merge", vec![patch]));
    }
    Ok((edit.then(DespawnEntity::new(layer)), below.id))
}

/// The lower layer's pixels with the upper one laid over them, as a patch
/// covering the upper layer's pixels.
fn merged_patch(
    upper: &RasterTile2D,
    style: LayerStyle,
    lower: &RasterTile2D,
    lower_offset: [i64; 2],
) -> Option<RasterPatch> {
    let ub = upper.bytes()?;
    // Where the upper layer's pixels land in the lower layer.
    let shift = [
        style.offset[0] - lower_offset[0],
        style.offset[1] - lower_offset[1],
    ];
    let mut bounds: Option<[u32; 4]> = None;
    for y in 0..upper.height() {
        for x in 0..upper.width() {
            if ub[((y * upper.width() + x) * 4 + 3) as usize] == 0 {
                continue;
            }
            let (lx, ly) = (i64::from(x) + shift[0], i64::from(y) + shift[1]);
            if lx < 0 || ly < 0 || lx >= i64::from(lower.width()) || ly >= i64::from(lower.height())
            {
                continue;
            }
            let (lx, ly) = (lx as u32, ly as u32);
            bounds = Some(match bounds {
                Some([a, b, c, d]) => [a.min(lx), b.min(ly), c.max(lx + 1), d.max(ly + 1)],
                None => [lx, ly, lx + 1, ly + 1],
            });
        }
    }
    let [x0, y0, x1, y1] = bounds?;
    let (width, height) = (x1 - x0, y1 - y0);
    let before = RasterPatch::read(lower, x0, y0, width, height)?;
    let mut after = before.clone();
    for y in 0..height {
        for x in 0..width {
            let (lx, ly) = (i64::from(x0 + x), i64::from(y0 + y));
            let (ux, uy) = (lx - shift[0], ly - shift[1]);
            if ux < 0 || uy < 0 || ux >= i64::from(upper.width()) || uy >= i64::from(upper.height())
            {
                continue;
            }
            let at = ((uy as u32 * upper.width() + ux as u32) * 4) as usize;
            let mut source = unit(&ub[at..at + 4]);
            source[3] *= style.opacity;
            let here = ((y * width + x) * 4) as usize;
            let out = style.blend.composite(unit(&after[here..here + 4]), source);
            after[here..here + 4].copy_from_slice(&bytes(out));
        }
    }
    Some(RasterPatch {
        x: x0,
        y: y0,
        width,
        height,
        before,
        after,
    })
}

/// Straight RGBA8 as 0 to 1.
#[must_use]
pub fn unit(pixel: &[u8]) -> [f32; 4] {
    [
        f32::from(pixel[0]) / 255.0,
        f32::from(pixel[1]) / 255.0,
        f32::from(pixel[2]) / 255.0,
        f32::from(pixel[3]) / 255.0,
    ]
}

/// 0 to 1 as straight RGBA8.
#[must_use]
pub fn bytes(color: [f32; 4]) -> [u8; 4] {
    color.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Composites the visible layers into a new raster layer on top (`flatten`
/// false), or replaces every layer with that one layer (`flatten` true).
///
/// # Errors
/// If the picture has no size.
pub fn merge_visible(
    arena: &Arena,
    picture: EntityId,
    flatten: bool,
) -> Result<(Edit, EntityId), String> {
    let [width, height] = size_of(arena, picture)?;
    let pixels = render(
        arena,
        picture,
        [0, 0, width, height],
        RenderOptions::default(),
    );
    let raster = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())?;
    let stack = layers(arena, picture);
    let name = if flatten {
        "Layer 1".to_owned()
    } else {
        "Merged".to_owned()
    };
    let layer = Entity::new(name, raster).with_class(class::LAYER);
    let id = layer.id();
    let mut edit = Edit::new(if flatten {
        "Flatten Image"
    } else {
        "Merge Visible to New Layer"
    });
    if flatten {
        for info in &stack {
            edit = edit.then(DespawnEntity::new(info.id));
        }
        edit = edit.then(
            SpawnEntity::new(layer)
                .with_relation(Relation::new(picture, CHILD, id).ordered(OrderKey::default())),
        );
    } else {
        let order = arena.relations().next_order(picture, CHILD);
        edit = edit.then(
            SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        );
    }
    Ok((edit, id))
}

/// A whole-layer edit: `change` rewrites a copy of the layer's pixels (as
/// straight RGBA8, with the layer's width) and the result becomes one
/// patch over the rectangle that changed.
///
/// # Errors
/// If the layer is not an 8-bit RGBA raster.
pub fn rewrite(
    arena: &Arena,
    layer: EntityId,
    label: &str,
    change: impl FnOnce(&mut [u8], u32, u32),
) -> Result<Option<RasterEdit>, String> {
    let source = raster(arena, layer).ok_or("Only raster layers can change")?;
    if source.format() != PixelFormat::Rgba8 {
        return Err("Only RGBA layers can change".to_owned());
    }
    let (width, height) = (source.width(), source.height());
    let before = source.bytes().ok_or("The layer is not 8-bit")?;
    let mut after = before.to_vec();
    change(&mut after, width, height);
    Ok(diff_patch(before, &after, width, height)
        .map(|patch| RasterEdit::new(layer, label, vec![patch])))
}

/// The smallest patch that turns `before` into `after` (both full layers).
#[must_use]
pub fn diff_patch(before: &[u8], after: &[u8], width: u32, height: u32) -> Option<RasterPatch> {
    let mut bounds: Option<[u32; 4]> = None;
    for y in 0..height {
        let row = (y * width * 4) as usize..((y + 1) * width * 4) as usize;
        let (a, b) = (&before[row.clone()], &after[row]);
        if a == b {
            continue;
        }
        let first = a.iter().zip(b).position(|(x, y)| x != y).unwrap_or(0) as u32 / 4;
        let last = a.iter().zip(b).rposition(|(x, y)| x != y).unwrap_or(0) as u32 / 4 + 1;
        bounds = Some(match bounds {
            Some([x0, y0, x1, _]) => [x0.min(first), y0, x1.max(last), y + 1],
            None => [first, y, last, y + 1],
        });
    }
    let [x0, y0, x1, y1] = bounds?;
    let cut = |pixels: &[u8]| {
        let mut out = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
        for y in y0..y1 {
            let at = ((y * width + x0) * 4) as usize;
            out.extend_from_slice(&pixels[at..at + ((x1 - x0) * 4) as usize]);
        }
        out
    };
    Some(RasterPatch {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
        before: cut(before),
        after: cut(after),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{ReversibleTransaction, UndoStack};

    #[test]
    fn a_quarter_turn_swaps_the_sides_and_four_come_back() {
        let mut arena = Arena::new();
        let (mut edit, picture) = create(&NewPicture {
            name: "Turn".to_owned(),
            width: 4,
            height: 2,
            ..NewPicture::default()
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let mut pixels = vec![0_u8; 4 * 2 * 4];
        pixels[..4].copy_from_slice(&[9, 9, 9, 255]);
        let raster = RasterTile2D::from_bytes(4, 2, PixelFormat::Rgba8, pixels).unwrap();
        let (mut spawn, layer) =
            add_layer_pixels(&arena, picture, None, "Dot".to_owned(), raster).unwrap();
        spawn.apply(&mut arena).unwrap();
        let before = raster_of(&arena, layer);
        let mut turn = rotate_canvas(&arena, picture, true).unwrap();
        turn.apply(&mut arena).unwrap();
        assert_eq!(size_of(&arena, picture).unwrap(), [2, 4]);
        // The top left corner lands top right.
        assert_eq!(&raster_of(&arena, layer)[4..8], &[9, 9, 9, 255]);
        for _ in 0..3 {
            rotate_canvas(&arena, picture, true)
                .unwrap()
                .apply(&mut arena)
                .unwrap();
        }
        assert_eq!(raster_of(&arena, layer), before, "four turns are none");
        let mut back = rotate_canvas(&arena, picture, false).unwrap();
        back.apply(&mut arena).unwrap();
        back.revert(&mut arena).unwrap();
        assert_eq!(size_of(&arena, picture).unwrap(), [4, 2]);
    }

    fn raster_of(arena: &Arena, layer: EntityId) -> Vec<u8> {
        raster(arena, layer).unwrap().bytes().unwrap().to_vec()
    }

    #[test]
    fn merging_selected_layers_keeps_the_lowest_and_its_look() {
        let mut arena = Arena::new();
        let (mut edit, picture) = create(&NewPicture {
            name: "Merge".to_owned(),
            width: 2,
            height: 1,
            ..NewPicture::default()
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let layer = |arena: &mut Arena, rgba: [u8; 4], name: &str| {
            let raster =
                RasterTile2D::from_bytes(2, 1, PixelFormat::Rgba8, rgba.repeat(2)).unwrap();
            let top = layers(arena, picture).last().map(|info| info.id);
            let (mut spawn, id) =
                add_layer_pixels(arena, picture, top, name.to_owned(), raster).unwrap();
            spawn.apply(arena).unwrap();
            id
        };
        let low = layer(&mut arena, [255, 0, 0, 255], "Red");
        let high = layer(&mut arena, [0, 0, 255, 255], "Blue");
        SetProperty::new(high, picture::OPACITY, 0.5)
            .apply(&mut arena)
            .unwrap();
        assert!(
            merge_layers(&arena, picture, &[low]).is_err(),
            "two at least"
        );
        let (mut edit, into) = merge_layers(&arena, picture, &[low, high]).unwrap();
        edit.apply(&mut arena).unwrap();
        assert_eq!(into, low);
        assert!(arena.get_by_id(high).is_err());
        let merged = raster_of(&arena, low);
        assert!(
            merged[0] > 100 && merged[2] > 100 && merged[3] == 255,
            "half blue over red"
        );
        edit.revert(&mut arena).unwrap();
        assert!(arena.get_by_id(high).is_ok());
    }

    #[test]
    fn a_photo_becomes_a_picture_of_itself() {
        let mut arena = Arena::new();
        let photo = RasterTile2D::new(6, 4, PixelFormat::Rgba8).unwrap();
        let handle = arena.spawn("photo.png", photo);
        let layer = arena.get(handle).unwrap().id();
        let (mut edit, picture) = adopt(&arena, layer, "photo").unwrap();
        edit.apply(&mut arena).unwrap();
        assert_eq!(
            layers(&arena, picture)
                .iter()
                .map(|info| info.id)
                .collect::<Vec<_>>(),
            vec![layer]
        );
        assert_eq!(
            picture::size(arena.get_by_id(picture).unwrap()),
            Some([6, 4])
        );
        assert!(
            adopt(&arena, layer, "again").is_err(),
            "one picture at a time"
        );
        edit.revert(&mut arena).unwrap();
        assert!(arena.get_by_id(picture).is_err());
        assert!(arena.get_by_id(layer).is_ok(), "the photo stays");
    }

    fn fresh() -> (Arena, EntityId) {
        let mut arena = Arena::new();
        let (mut edit, picture) = create(&NewPicture {
            width: 16,
            height: 8,
            ..NewPicture::default()
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        (arena, picture)
    }

    #[test]
    fn a_new_picture_has_paper_and_one_layer_and_undoes_whole() {
        let mut arena = Arena::new();
        let (edit, picture) = create(&NewPicture::default()).unwrap();
        let mut history = UndoStack::new();
        history.apply(&mut arena, edit).unwrap();
        let stack = layers(&arena, picture);
        assert_eq!(
            stack
                .iter()
                .map(|layer| layer.name.as_str())
                .collect::<Vec<_>>(),
            ["Paper", "Layer 1"]
        );
        assert_eq!(stack[0].kind, LayerKind::Paper);
        assert!(picture::is_picture(arena.get_by_id(picture).unwrap()));
        history.undo(&mut arena).unwrap();
        assert!(arena.is_empty());
        history.redo(&mut arena).unwrap();
        assert_eq!(layers(&arena, picture).len(), 2);
        assert!(
            create(&NewPicture {
                width: 0,
                ..NewPicture::default()
            })
            .is_err()
        );
    }

    #[test]
    fn canvas_size_crop_and_resolution_keep_the_drawing() {
        let mut arena = Arena::new();
        let mut history: sue_core::UndoStack<Arena> = sue_core::UndoStack::new();
        let (edit, picture) = create(&NewPicture {
            width: 20,
            height: 10,
            paper: None,
            ..NewPicture::default()
        })
        .unwrap();
        history.apply(&mut arena, edit).unwrap();
        let layer = layers(&arena, picture)[0].id;
        if let Some(bytes) = raster_mut(&mut arena, layer).and_then(RasterTile2D::bytes_mut) {
            // One red pixel at (2, 3).
            bytes[(3 * 20 + 2) * 4..][..4].copy_from_slice(&[255, 0, 0, 255]);
        }
        let red_at = |arena: &Arena| {
            let tile = raster(arena, layer).unwrap();
            let bytes = tile.bytes().unwrap();
            (0..tile.width() * tile.height())
                .find(|index| bytes[(*index * 4) as usize] == 255)
                .map(|index| [index % tile.width(), index / tile.width()])
        };
        // A bigger canvas anchored in the middle: everything moves 5, 5.
        let edit = resize_canvas(&arena, picture, [30, 20], [5, 5]).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(
            picture::size(arena.get_by_id(picture).unwrap()),
            Some([30, 20])
        );
        assert_eq!(red_at(&arena), Some([7, 8]));
        // Crop to a box around it.
        let edit = resize_canvas(&arena, picture, [4, 4], [-6, -7]).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(red_at(&arena), Some([1, 1]));
        // Twice the resolution.
        let edit = resample(&arena, picture, [8, 8], 144.0).unwrap();
        history.apply(&mut arena, edit).unwrap();
        let tile = raster(&arena, layer).unwrap();
        assert_eq!(tile.width(), 8);
        let strongest = (0..64_u32)
            .max_by_key(|index| tile.bytes().unwrap()[(*index * 4 + 3) as usize])
            .unwrap();
        assert!(
            (2..=3).contains(&(strongest % 8)) && (2..=3).contains(&(strongest / 8)),
            "{strongest}"
        );
        // Undo all three: back to the start.
        for _ in 0..3 {
            history.undo(&mut arena).unwrap();
        }
        assert_eq!(red_at(&arena), Some([2, 3]));
        assert_eq!(
            picture::size(arena.get_by_id(picture).unwrap()),
            Some([20, 10])
        );
        assert_eq!(
            move_shapes("10,20 30,40;e:1,2,3,4", [5.0, -5.0], [2.0, 2.0]),
            "30,30 70,70;e:12,-6,16,-2"
        );
    }

    #[test]
    fn vector_layers_rasterize_in_place_and_reference_layers_are_marked() {
        let mut arena = Arena::new();
        let (mut edit, picture) = create(&NewPicture {
            width: 8,
            height: 8,
            ..NewPicture::default()
        })
        .unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let below = layers(&arena, picture).last().map(|info| info.id);
        let (mut spawn, vector) =
            add_vector_layer(&arena, picture, below, "Ink".to_owned()).unwrap();
        sue_core::ReversibleTransaction::apply(&mut spawn, &mut arena).unwrap();
        let mut opacity = set(vector, picture::OPACITY, 0.5);
        sue_core::ReversibleTransaction::apply(&mut opacity, &mut arena).unwrap();
        let before: Vec<String> = layers(&arena, picture)
            .into_iter()
            .map(|info| info.name)
            .collect();
        let (mut edit, raster_layer) = rasterize(&arena, picture, vector).unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let after = layers(&arena, picture);
        assert_eq!(
            after
                .iter()
                .map(|info| info.name.clone())
                .collect::<Vec<_>>(),
            before,
            "in the same place"
        );
        assert!(!crate::vector::is_vector(&arena, raster_layer));
        assert!(
            (after.last().unwrap().style.opacity - 0.5).abs() < 1e-6,
            "its properties kept"
        );
        assert!(rasterize(&arena, picture, raster_layer).is_err());
        sue_core::ReversibleTransaction::revert(&mut edit, &mut arena).unwrap();
        assert!(
            crate::vector::is_vector(&arena, vector),
            "undo brings the vector layer back"
        );
        // Set as Reference Layer.
        assert!(!is_reference(&arena, vector));
        let mut mark = set(vector, REFERENCE, true);
        sue_core::ReversibleTransaction::apply(&mut mark, &mut arena).unwrap();
        assert!(is_reference(&arena, vector));
    }

    #[test]
    fn correction_layers_change_the_picture_below_them() {
        let mut arena = Arena::new();
        let (mut edit, picture) = create(&NewPicture {
            width: 2,
            height: 1,
            paper: Some(Color::srgb8(200, 200, 200)),
            ..NewPicture::default()
        })
        .unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let shown =
            |arena: &Arena| render(arena, picture, [0, 0, 1, 1], RenderOptions::default())[0];
        assert_eq!(shown(&arena), 200);
        let top = layers(&arena, picture).last().map(|info| info.id);
        let (mut edit, correction) =
            new_correction(&arena, picture, top, "Reverse Gradient", "invert");
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        assert_eq!(shown(&arena), 55, "inverted below it");
        let mut half = set(correction, picture::OPACITY, 0.5);
        sue_core::ReversibleTransaction::apply(&mut half, &mut arena).unwrap();
        assert_eq!(shown(&arena), 128, "halfway at half opacity");
    }

    #[test]
    fn masks_hide_part_of_a_layer_and_apply_to_it() {
        let mut arena = Arena::new();
        let mut history: sue_core::UndoStack<Arena> = sue_core::UndoStack::new();
        let (edit, picture) = create(&NewPicture {
            width: 4,
            height: 1,
            paper: None,
            ..NewPicture::default()
        })
        .unwrap();
        history.apply(&mut arena, edit).unwrap();
        let layer = layers(&arena, picture)[0].id;
        if let Some(bytes) = raster_mut(&mut arena, layer).and_then(RasterTile2D::bytes_mut) {
            bytes.fill(255);
        }
        let shown = |arena: &Arena| -> Vec<u8> {
            render(arena, picture, [0, 0, 4, 1], RenderOptions::default())
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| pixel[3])
                .collect()
        };
        // Show only the left half.
        let (edit, mask) = new_mask(&arena, layer, |x, _| if x < 2 { 1.0 } else { 0.0 }).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(mask_of(&arena, layer), Some(mask));
        assert_eq!(shown(&arena), vec![255, 255, 0, 0]);
        assert!(
            new_mask(&arena, layer, |_, _| 1.0).is_err(),
            "one mask a layer"
        );
        // Off, the whole layer shows.
        history
            .apply(&mut arena, set(layer, MASK_ON, false))
            .unwrap();
        assert_eq!(shown(&arena), vec![255; 4]);
        history.undo(&mut arena).unwrap();
        // Applied: the hidden half is gone for good, and so is the mask.
        let edit = apply_mask(&arena, layer).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(mask_of(&arena, layer), None);
        assert_eq!(shown(&arena), vec![255, 255, 0, 0]);
        history.undo(&mut arena).unwrap();
        assert_eq!(
            mask_of(&arena, layer),
            Some(mask),
            "undo brings the mask back"
        );
        let edit = delete_mask(&arena, layer).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(shown(&arena), vec![255; 4]);
    }

    #[test]
    fn folders_hold_layers_hide_them_and_go_with_them() {
        let mut arena = Arena::new();
        let mut history: sue_core::UndoStack<Arena> = sue_core::UndoStack::new();
        let (edit, picture) = create(&NewPicture {
            width: 4,
            height: 4,
            paper: None,
            ..NewPicture::default()
        })
        .unwrap();
        history.apply(&mut arena, edit).unwrap();
        let layer = layers(&arena, picture)[0].id;
        if let Some(bytes) = raster_mut(&mut arena, layer).and_then(RasterTile2D::bytes_mut) {
            bytes.fill(255);
        }
        // Ctrl+G: a folder right above the layer, with the layer in it.
        let (edit, folder) = new_folder(&arena, picture, Some(layer), true);
        history.apply(&mut arena, edit).unwrap();
        assert!(is_folder(&arena, folder));
        assert_eq!(folder_of(&arena, layer), Some(folder));
        assert_eq!(members(&arena, picture, folder), vec![layer]);
        let stack: Vec<EntityId> = layers(&arena, picture).iter().map(|info| info.id).collect();
        assert_eq!(
            stack,
            vec![layer, folder],
            "the folder sits above its layer"
        );
        // A folder inside it: the layer is in both.
        let (edit, inner) = new_folder(&arena, picture, Some(layer), true);
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(folders_around(&arena, layer), vec![inner, folder]);
        // Hiding or fading the folder hides or fades what is in it.
        let shown =
            |arena: &Arena| render(arena, picture, [0, 0, 1, 1], RenderOptions::default())[3];
        assert_eq!(shown(&arena), 255);
        history
            .apply(&mut arena, set(folder, picture::OPACITY, 0.5))
            .unwrap();
        assert_eq!(shown(&arena), 128);
        history
            .apply(&mut arena, set(folder, picture::VISIBLE, false))
            .unwrap();
        assert_eq!(shown(&arena), 0);
        // Out of the folders, then the outer folder goes with its layers.
        let out = set_folder(&arena, layer, None).unwrap();
        history.apply(&mut arena, out).unwrap();
        assert_eq!(folders_around(&arena, layer), Vec::<EntityId>::new());
        history.undo(&mut arena).unwrap();
        let edit = delete_folder(&arena, picture, folder);
        history.apply(&mut arena, edit).unwrap();
        assert!(arena.get_by_id(layer).is_err() && arena.get_by_id(inner).is_err());
        history.undo(&mut arena).unwrap();
        assert_eq!(
            folders_around(&arena, layer),
            vec![inner, folder],
            "undo brings it all back"
        );
    }

    #[test]
    fn layers_are_added_duplicated_reordered_and_deleted() {
        let (mut arena, picture) = fresh();
        let first = layers(&arena, picture)[1].id;
        assert_eq!(new_layer_name(&arena, picture), "Layer 2");
        let (mut spawn, second) =
            add_raster_layer(&arena, picture, Some(first), "Layer 2".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        // A layer added above the first lands between it and the top.
        let (mut middle, between) =
            add_raster_layer(&arena, picture, Some(first), "Between".to_owned(), None).unwrap();
        middle.apply(&mut arena).unwrap();
        let names = |arena: &Arena| {
            layers(arena, picture)
                .into_iter()
                .map(|layer| layer.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&arena), ["Paper", "Layer 1", "Between", "Layer 2"]);
        let (mut copy, _) = duplicate(&arena, picture, second).unwrap();
        copy.apply(&mut arena).unwrap();
        assert_eq!(names(&arena)[4], "Layer 2 copy");
        let mut down = reorder(&arena, picture, second, 1).unwrap();
        down.apply(&mut arena).unwrap();
        assert_eq!(
            names(&arena),
            ["Paper", "Layer 2", "Layer 1", "Between", "Layer 2 copy"]
        );
        down.revert(&mut arena).unwrap();
        assert_eq!(names(&arena)[3], "Layer 2");
        assert!(
            reorder(&arena, picture, second, 3).is_none(),
            "already there"
        );
        assert_eq!(label(picture::VISIBLE, &false.into()), "Hide Layer");
        assert_eq!(set(second, picture::OPACITY, 0.5).label(), "Layer Opacity");
        let mut gone = delete(between);
        gone.apply(&mut arena).unwrap();
        assert_eq!(names(&arena).len(), 4);
        gone.revert(&mut arena).unwrap();
        assert_eq!(names(&arena)[2], "Between");
    }

    #[test]
    fn merging_down_lays_the_layer_over_the_one_below() {
        let (mut arena, picture) = fresh();
        let base = layers(&arena, picture)[1].id;
        let (mut spawn, top) =
            add_raster_layer(&arena, picture, Some(base), "Top".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        raster_mut(&mut arena, base).unwrap().bytes_mut().unwrap()[..4]
            .copy_from_slice(&[0, 0, 255, 255]);
        raster_mut(&mut arena, top).unwrap().bytes_mut().unwrap()[..8]
            .copy_from_slice(&[255, 0, 0, 255, 255, 0, 0, 128]);
        let (edit, into) = merge_down(&arena, picture, top).unwrap();
        assert_eq!(into, base);
        let mut history = UndoStack::new();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(layers(&arena, picture).len(), 2);
        let pixels = raster(&arena, base).unwrap().bytes().unwrap();
        assert_eq!(&pixels[..8], &[255, 0, 0, 255, 255, 0, 0, 128]);
        history.undo(&mut arena).unwrap();
        assert_eq!(layers(&arena, picture).len(), 3);
        assert_eq!(
            &raster(&arena, base).unwrap().bytes().unwrap()[..4],
            &[0, 0, 255, 255]
        );
        // The paper takes no merge.
        assert!(merge_down(&arena, picture, base).is_err());
    }

    #[test]
    fn flattening_and_rewrites_keep_only_what_changed() {
        let (mut arena, picture) = fresh();
        let layer = layers(&arena, picture)[1].id;
        let edit = rewrite(&arena, layer, "Fill", |pixels, width, _| {
            let at = ((2 * width + 3) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[1, 2, 3, 255]);
        })
        .unwrap()
        .unwrap();
        assert_eq!(
            (
                edit.patches()[0].x,
                edit.patches()[0].y,
                edit.patches()[0].width
            ),
            (3, 2, 1)
        );
        assert!(
            rewrite(&arena, layer, "Nothing", |_, _, _| {})
                .unwrap()
                .is_none()
        );
        let (mut flat, merged) = merge_visible(&arena, picture, true).unwrap();
        flat.apply(&mut arena).unwrap();
        let stack = layers(&arena, picture);
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].id, merged);
        assert_eq!(
            &raster(&arena, merged).unwrap().bytes().unwrap()[..4],
            &[255, 255, 255, 255],
            "the paper is in the flattened layer"
        );
        flat.revert(&mut arena).unwrap();
        assert_eq!(layers(&arena, picture).len(), 2);
    }
}
