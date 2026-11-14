//! Pictures in the arena and the undoable edits Sue Paint makes to them.
//!
//! Every picture is the shared layered-picture shape of
//! [`sue_model::picture`]: an entity of class `sue.image`, its layers as
//! ordered children. Each function here builds a transaction from the
//! backbone's generic edits ([`SpawnEntity`],
//! [`DespawnEntity`](sue_core::txn::DespawnEntity), [`SetProperty`],
//! [`Relate`], [`RasterEdit`]) by entity id; the studio applies it
//! through the global undo history.

mod convert;
mod folders;
mod kinds;
mod layers;
mod lines_tones;
mod masks;
mod order;
mod palette;
mod picked;
#[cfg(test)]
mod picked_tests;

pub use convert::{
    fitted, rasterize, resample, resize_canvas, resize_rgba, rotate_canvas, vectorize,
};
pub use folders::{
    MEMBER, OPEN, delete_folder, folder_blend, folder_of, folders_around, is_folder, is_open,
    members, new_folder, set_folder,
};
pub use kinds::{
    FILL_COLOR, FILL_LAYER, GRADIENT_END, GRADIENT_FROM_X, GRADIENT_FROM_Y, GRADIENT_LAYER,
    GRADIENT_RADIAL, GRADIENT_START, GRADIENT_TO_X, GRADIENT_TO_Y, Gradient, MATERIAL_ANGLE,
    MATERIAL_IMAGE, MATERIAL_IMAGE_CLASS, MATERIAL_LAYER, MATERIAL_SCALE, MATERIAL_TILE,
    MATERIAL_X, MATERIAL_Y, Placement, material_image, new_fill_layer, new_gradient_layer,
    new_material_layer,
};
pub use layers::{
    CORRECTION, CORRECTION_FILTER, REFERENCE, TEXT, add_layer_pixels, add_layers_pixels, add_paper,
    add_raster_layer, add_raster_layer_at, add_vector_layer, add_vector_lines, delete, duplicate,
    is_reference, merge_down, merge_layers, merge_visible, new_correction, transfer_down,
};
pub use lines_tones::{LinesAndTones, lines_and_tones};
pub use masks::{
    MASK, MASK_CLASS, MASK_LINKED, MASK_ON, apply_mask, cover, delete_mask, mask_linked, mask_of,
    mask_offset, mask_on, new_mask, set_mask_linked, shown_at,
};
pub(crate) use order::key_above;
pub use order::{key_below, keys_above, landing, move_block, reorder};
pub use palette::{
    PALETTE_COLOR, PALETTE_COLORS, named_palette_color, palette_color, set_palette_color,
};
pub use picked::{
    delete_layers, duplicate_layers, folder_insert, merge_picked, move_layers, outermost,
    rasterize_any, rasterizes, set_layers, with_members,
};

use sue_core::txn::{Batch, RasterEdit, RasterPatch, Relate, SetProperty, SpawnEntity};
use sue_core::{
    Arena, CHILD, Color, ComponentData, ComponentKind, Entity, EntityId, OrderKey, PixelFormat,
    RasterTile2D, Relation, Value,
};
use sue_model::class;
use sue_model::picture::{self, LayerKind, LayerStyle};

/// An edit of a picture: several arena steps undone as one.
pub type Edit = Batch<Arena>;

/// The largest side of a new picture, in pixels.
pub const MAX_SIDE: u32 = 20_000;

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
            name: tr!("illustration"),
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
        return Err(tr!(
            "picture-must-1-pixels-side-not",
            width = spec.width,
            height = spec.height,
            MAX_SIDE = MAX_SIDE
        ));
    }
    let image = Entity::empty(spec.name.clone(), class::IMAGE)
        .with_property(picture::WIDTH, i64::from(spec.width))
        .with_property(picture::HEIGHT, i64::from(spec.height))
        .with_property(picture::DPI, spec.dpi)
        .with_property(crate::timelapse::RECORD, spec.timelapse);
    let id = image.id();
    let mut edit = Edit::new(tr!("new", name = spec.name)).then(SpawnEntity::new(image));
    let mut keys = OrderKey::sequence(None, 2).into_iter();
    if let Some(color) = spec.paper {
        let paper =
            Entity::empty(tr!("paper-2"), class::PAPER).with_property(picture::COLOR, color);
        let order = keys.next().unwrap_or_default();
        let paper_id = paper.id();
        edit = edit.then(
            SpawnEntity::new(paper)
                .with_relation(Relation::new(id, CHILD, paper_id).ordered(order)),
        );
    }
    let raster = RasterTile2D::new(spec.width, spec.height, PixelFormat::Rgba8)
        .map_err(|error| error.to_string())?;
    let layer = Entity::new(tr!("layer-number", number = 1), raster).with_class(class::LAYER);
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
    let raster = raster(arena, layer).ok_or(tr!("has-no-pixels-paint"))?;
    if raster.format() != PixelFormat::Rgba8 {
        return Err(tr!("only-8-bit-color-pictures-can"));
    }
    if arena.relations().parent(layer, CHILD).is_some() {
        return Err(tr!("already-layer-picture"));
    }
    let (width, height) = (raster.width(), raster.height());
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(tr!("picture-must-most-pixels-side", MAX_SIDE = MAX_SIDE));
    }
    let image = Entity::empty(name, class::IMAGE)
        .with_property(picture::WIDTH, i64::from(width))
        .with_property(picture::HEIGHT, i64::from(height))
        .with_property(picture::DPI, 72.0);
    let id = image.id();
    let edit = Edit::new(tr!("open-in-sue-paint", name = name))
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

/// A name like `Layer 3`, in the language in use, that no layer of
/// `picture` has yet (whatever language the others were named in, the
/// number at their end counts).
#[must_use]
pub fn new_layer_name(arena: &Arena, picture: EntityId) -> String {
    let highest = layers(arena, picture)
        .iter()
        .filter_map(|layer| layer.name.rsplit(' ').next()?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    tr!("layer-number", number = highest + 1)
}

/// The size of a picture.
fn size_of(arena: &Arena, picture: EntityId) -> Result<[u32; 2], String> {
    arena
        .get_by_id(picture)
        .ok()
        .and_then(picture::size)
        .ok_or_else(|| tr!("the-picture-has-no-size"))
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
        picture::VISIBLE => pick(&tr!("show-layer"), &tr!("hide-layer")),
        picture::LOCKED => pick(&tr!("lock-layer"), &tr!("unlock-layer")),
        picture::ALPHA_LOCKED => pick(
            &tr!("lock-transparent-pixels"),
            &tr!("unlock-transparent-pixels"),
        ),
        picture::CLIP => pick(&tr!("clip-to-layer-below"), &tr!("release-clipping")),
        picture::DRAFT => pick(&tr!("set-as-draft-layer"), &tr!("unset-draft-layer")),
        picture::OPACITY => tr!("layer-opacity"),
        picture::BLEND => tr!("blend-mode"),
        picture::COLOR => tr!("paper-color-2"),
        picture::X | picture::Y => tr!("move-layer"),
        crate::effects::BORDER => pick(&tr!("border-effect"), &tr!("no-border-effect")),
        crate::effects::BORDER_WIDTH | crate::effects::BORDER_COLOR => tr!("border-effect"),
        crate::effects::TONE => pick(&tr!("tone"), &tr!("no-tone")),
        crate::effects::TONE_LINES
        | crate::effects::TONE_ANGLE
        | crate::effects::TONE_SOURCE
        | crate::effects::TONE_LEVELS
        | crate::effects::TONE_SHAPE => tr!("tone"),
        crate::effects::COLOR => pick(&tr!("layer-color"), &tr!("no-layer-color")),
        crate::effects::COLOR_VALUE | crate::effects::COLOR_SUB => tr!("layer-color"),
        PALETTE_COLOR => tr!("change-palette-color"),
        MASK_LINKED => pick(&tr!("link-mask-to-layer"), &tr!("unlink-mask-from-layer")),
        crate::effects::EXPRESSION | crate::effects::THRESHOLD => tr!("expression-color"),
        REFERENCE => pick(
            &tr!("set-as-reference-layer"),
            &tr!("unset-reference-layer"),
        ),
        other => tr!("set", other = other),
    }
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
    let source = raster(arena, layer).ok_or(tr!("only-raster-layers-can-change"))?;
    if source.format() != PixelFormat::Rgba8 {
        return Err(tr!("only-rgba-layers-can-change"));
    }
    let (width, height) = (source.width(), source.height());
    let before = source.bytes().ok_or(tr!("layer-not-8-bit"))?;
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
    use super::convert::move_shapes;
    use super::*;
    use crate::composite::{RenderOptions, render};
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
