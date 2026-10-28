//! Layered pictures: the shape every raster feature agrees on.
//!
//! A picture is an entity of class [`crate::class::IMAGE`] with no base
//! component, its size in [`WIDTH`] and [`HEIGHT`], and its layers as
//! ordered `sue.child` relations, bottom first. A layer is any child with a
//! `RasterTile2D` base (a painted layer, or a photo that also sits on the
//! canvas: the same entity, never a copy) or a [`crate::class::PAPER`]
//! entity filled with one color. How a layer is laid over the ones below is
//! read with [`LayerStyle::of`] from shared properties, so a painting
//! studio, a photo editor and a video compositor stack it the same way.

use sue_core::{Arena, Color, ComponentKind, Entity, EntityId};

use crate::{BlendMode, class};

/// Picture width in pixels (`Int`).
pub const WIDTH: &str = "image.width";
/// Picture height in pixels (`Int`).
pub const HEIGHT: &str = "image.height";
/// Print resolution in pixels per inch (`Float`).
pub const DPI: &str = "image.dpi";
/// Layer opacity, 0 to 1 (`Float`).
pub const OPACITY: &str = "style.opacity";
/// Layer blend mode by [`BlendMode::name`] (`Text`).
pub const BLEND: &str = "style.blend";
/// `false` hides a layer (`Bool`).
pub const VISIBLE: &str = "layer.visible";
/// `true` keeps a layer from being edited (`Bool`).
pub const LOCKED: &str = "layer.locked";
/// `true` keeps painting inside a layer's existing pixels (`Bool`).
pub const ALPHA_LOCKED: &str = "layer.alpha_locked";
/// `true` shows a layer only where the layer below it has pixels (`Bool`).
pub const CLIP: &str = "layer.clip";
/// `true` marks a sketch layer left out of exports (`Bool`).
pub const DRAFT: &str = "layer.draft";
/// The fill of a paper layer (`Color`).
pub const COLOR: &str = "layer.color";
/// Horizontal offset of a layer's pixels in the picture (`Int`).
pub const X: &str = "layer.x";
/// Vertical offset of a layer's pixels in the picture (`Int`).
pub const Y: &str = "layer.y";

/// `true` if `entity` is a layered picture.
#[must_use]
pub fn is_picture(entity: &Entity) -> bool {
    entity.class() == class::IMAGE && entity.base_kind().is_none()
}

/// A picture's size in pixels, if it has one.
#[must_use]
pub fn size(entity: &Entity) -> Option<[u32; 2]> {
    let read = |key| {
        entity
            .property(key)
            .and_then(sue_core::Value::as_i64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0)
    };
    Some([read(WIDTH)?, read(HEIGHT)?])
}

/// The layers of `picture`, bottom first.
#[must_use]
pub fn layers(arena: &Arena, picture: EntityId) -> Vec<EntityId> {
    arena.relations().children(picture)
}

/// The picture a layer belongs to.
#[must_use]
pub fn picture_of(arena: &Arena, layer: EntityId) -> Option<EntityId> {
    let parent = arena.relations().parent(layer, sue_core::CHILD)?;
    arena
        .get_by_id(parent)
        .is_ok_and(is_picture)
        .then_some(parent)
}

/// What a layer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    /// Pixels: a `RasterTile2D` base.
    Raster,
    /// One color over the whole picture.
    Paper,
    /// Anything else (a newer layer kind, a folder): kept, not drawn.
    Other,
}

impl LayerKind {
    /// The kind of `entity`.
    #[must_use]
    pub fn of(entity: &Entity) -> Self {
        if entity.base_kind() == Some(ComponentKind::RasterTile2D) {
            Self::Raster
        } else if entity.class() == class::PAPER {
            Self::Paper
        } else {
            Self::Other
        }
    }
}

/// How a layer is laid over the layers below it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerStyle {
    /// Drawn at all.
    pub visible: bool,
    /// 0 to 1.
    pub opacity: f32,
    /// How its color mixes with the backdrop.
    pub blend: BlendMode,
    /// Shown only where the nearest unclipped layer below has pixels.
    pub clip: bool,
    /// Where its pixels start in the picture.
    pub offset: [i64; 2],
    /// Left out of exports.
    pub draft: bool,
}

impl Default for LayerStyle {
    fn default() -> Self {
        Self {
            visible: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            clip: false,
            offset: [0, 0],
            draft: false,
        }
    }
}

impl LayerStyle {
    /// Reads the style of `entity`; missing properties take the defaults.
    #[must_use]
    pub fn of(entity: &Entity) -> Self {
        let flag = |key, default| {
            entity
                .property(key)
                .and_then(sue_core::Value::as_bool)
                .unwrap_or(default)
        };
        let int = |key| {
            entity
                .property(key)
                .and_then(sue_core::Value::as_i64)
                .unwrap_or(0)
        };
        Self {
            visible: flag(VISIBLE, true),
            opacity: entity
                .property(OPACITY)
                .and_then(sue_core::Value::as_f64)
                .map_or(1.0, |value| value.clamp(0.0, 1.0) as f32),
            blend: entity
                .property(BLEND)
                .and_then(sue_core::Value::as_str)
                .map_or(BlendMode::Normal, BlendMode::from_name_or_normal),
            clip: flag(CLIP, false),
            offset: [int(X), int(Y)],
            draft: flag(DRAFT, false),
        }
    }
}

/// The fill of a paper layer (white if it has none).
#[must_use]
pub fn paper_color(entity: &Entity) -> Color {
    match entity.property(COLOR) {
        Some(sue_core::Value::Color(color)) => *color,
        _ => Color::srgb8(255, 255, 255),
    }
}

/// `true` if a layer is locked against edits.
#[must_use]
pub fn is_locked(entity: &Entity) -> bool {
    entity
        .property(LOCKED)
        .and_then(sue_core::Value::as_bool)
        .unwrap_or(false)
}

/// `true` if painting keeps a layer's alpha.
#[must_use]
pub fn is_alpha_locked(entity: &Entity) -> bool {
    entity
        .property(ALPHA_LOCKED)
        .and_then(sue_core::Value::as_bool)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{CHILD, PixelFormat, RasterTile2D, Relation, SueFile};

    #[test]
    fn a_picture_keeps_its_layers_in_order_through_a_file() {
        let mut arena = Arena::new();
        let picture = Entity::empty("Poster", class::IMAGE)
            .with_property(WIDTH, 4_i64)
            .with_property(HEIGHT, 3_i64)
            .with_property(DPI, 300.0);
        let picture_id = picture.id();
        arena.insert_keep_id(picture).unwrap();
        let paper =
            Entity::empty("Paper", class::PAPER).with_property(COLOR, Color::srgb8(250, 240, 230));
        let ink = Entity::new("Ink", RasterTile2D::new(4, 3, PixelFormat::Rgba8).unwrap())
            .with_class(class::LAYER)
            .with_property(BLEND, "multiply")
            .with_property(OPACITY, 0.5)
            .with_property(CLIP, true)
            .with_property(X, -2_i64);
        let future = Entity::empty("Folder", "sue.layer.folder-from-the-future");
        let ids = [paper.id(), ink.id(), future.id()];
        for entity in [paper, ink, future] {
            let id = entity.id();
            arena.insert_keep_id(entity).unwrap();
            let order = arena.relations().next_order(picture_id, CHILD);
            arena
                .relate(Relation::new(picture_id, CHILD, id).ordered(order))
                .unwrap();
        }

        let mut file = SueFile::new();
        file.set_arena(&arena);
        let loaded = SueFile::from_bytes(&file.to_bytes())
            .unwrap()
            .arena(&crate::registry())
            .unwrap();
        let entity = loaded.get_by_id(picture_id).unwrap();
        assert!(is_picture(entity));
        assert_eq!(size(entity), Some([4, 3]));
        assert_eq!(layers(&loaded, picture_id), ids.to_vec());
        assert_eq!(picture_of(&loaded, ids[1]), Some(picture_id));
        let kinds: Vec<LayerKind> = ids
            .iter()
            .map(|id| LayerKind::of(loaded.get_by_id(*id).unwrap()))
            .collect();
        assert_eq!(
            kinds,
            [LayerKind::Paper, LayerKind::Raster, LayerKind::Other]
        );
        let style = LayerStyle::of(loaded.get_by_id(ids[1]).unwrap());
        assert_eq!(style.blend, BlendMode::Multiply);
        assert!((style.opacity - 0.5).abs() < 1e-6);
        assert!(style.clip && style.visible && !style.draft);
        assert_eq!(style.offset, [-2, 0]);
        assert_eq!(
            paper_color(loaded.get_by_id(ids[0]).unwrap()),
            Color::srgb8(250, 240, 230)
        );
        // A plain raster is a picture's layer, not a picture.
        assert!(!is_picture(loaded.get_by_id(ids[1]).unwrap()));
    }
}
