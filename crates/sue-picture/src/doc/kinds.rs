//! Layers that stay live: a fill layer (one color), a gradient layer and
//! an image material layer (a picture placed, scaled, turned and tiled).
//! Each is a layer entity of its own class whose settings are properties,
//! drawn by the compositor as it goes, so they can be changed again at
//! any time; Rasterize turns them into pixels. A Sue that does not know a
//! class keeps the layer and leaves it out of the picture (Rule 3).

use sue_core::txn::SpawnEntity;
use sue_core::{Arena, Color, ColorSpace, Entity, EntityId, RasterTile2D, Relation, Value};

use super::Edit;
use super::layers::landed;

/// The class of a fill layer: one color over the whole picture.
pub const FILL_LAYER: &str = "sue.paint.fill_layer";
/// A fill layer's color (`Color`).
pub const FILL_COLOR: &str = "fill.color";

/// The class of a gradient layer.
pub const GRADIENT_LAYER: &str = "sue.paint.gradient_layer";
/// Where a gradient starts and ends, in picture pixels (`Float`s).
pub const GRADIENT_FROM_X: &str = "gradient.from_x";
/// See [`GRADIENT_FROM_X`].
pub const GRADIENT_FROM_Y: &str = "gradient.from_y";
/// See [`GRADIENT_FROM_X`].
pub const GRADIENT_TO_X: &str = "gradient.to_x";
/// See [`GRADIENT_FROM_X`].
pub const GRADIENT_TO_Y: &str = "gradient.to_y";
/// The color where a gradient starts (`Color`, with its alpha).
pub const GRADIENT_START: &str = "gradient.start";
/// The color where a gradient ends (`Color`, with its alpha).
pub const GRADIENT_END: &str = "gradient.end";
/// Round from the start instead of straight across (`Bool`).
pub const GRADIENT_RADIAL: &str = "gradient.radial";

/// The class of an image material layer: the picture its
/// [`MATERIAL_IMAGE`] relation names, placed on the canvas.
pub const MATERIAL_LAYER: &str = "sue.paint.material_layer";
/// The relation from an image material layer to the raster it shows.
pub const MATERIAL_IMAGE: &str = "sue.paint.material_image";
/// The class of that raster.
pub const MATERIAL_IMAGE_CLASS: &str = "sue.paint.material_image";
/// Where the image's middle sits, in picture pixels (`Float`s).
pub const MATERIAL_X: &str = "material.x";
/// See [`MATERIAL_X`].
pub const MATERIAL_Y: &str = "material.y";
/// How large the image shows, 1 as it is (`Float`).
pub const MATERIAL_SCALE: &str = "material.scale";
/// How far the image is turned, in degrees clockwise (`Float`).
pub const MATERIAL_ANGLE: &str = "material.angle";
/// The image repeats over the whole canvas (`Bool`).
pub const MATERIAL_TILE: &str = "material.tile";

/// A gradient as the compositor draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gradient {
    /// Where it starts and ends, in picture pixels.
    pub line: [[f32; 2]; 2],
    /// Its colors there (straight RGBA, 0 to 1).
    pub colors: [[f32; 4]; 2],
    /// Round from the start instead of straight across.
    pub radial: bool,
}

impl Gradient {
    /// The gradient a gradient layer holds.
    #[must_use]
    pub fn of(entity: &Entity) -> Self {
        let number = |key: &str| entity.property(key).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let color = |key: &str, or: [f32; 4]| match entity.property(key) {
            Some(Value::Color(color)) => {
                let srgb = color.to_space(ColorSpace::Srgb);
                [srgb.rgb[0], srgb.rgb[1], srgb.rgb[2], srgb.alpha]
            }
            _ => or,
        };
        Self {
            line: [
                [number(GRADIENT_FROM_X), number(GRADIENT_FROM_Y)],
                [number(GRADIENT_TO_X), number(GRADIENT_TO_Y)],
            ],
            colors: [
                color(GRADIENT_START, [0.0, 0.0, 0.0, 1.0]),
                color(GRADIENT_END, [1.0, 1.0, 1.0, 1.0]),
            ],
            radial: entity
                .property(GRADIENT_RADIAL)
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    /// The color at picture pixel `(x, y)` (its middle).
    #[must_use]
    pub fn at(&self, x: i64, y: i64) -> [f32; 4] {
        let [from, to] = self.line;
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let length_sq = (dx * dx + dy * dy).max(1e-6);
        let t = if self.radial {
            (((px - from[0]).powi(2) + (py - from[1]).powi(2)) / length_sq).sqrt()
        } else {
            ((px - from[0]) * dx + (py - from[1]) * dy) / length_sq
        }
        .clamp(0.0, 1.0);
        let [start, end] = self.colors;
        std::array::from_fn(|channel| start[channel] + (end[channel] - start[channel]) * t)
    }
}

/// How an image material layer places its image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Where the image's middle sits, in picture pixels.
    pub at: [f32; 2],
    /// How large it shows.
    pub scale: f32,
    /// How far it is turned, in degrees clockwise.
    pub angle: f32,
    /// It repeats over the whole canvas.
    pub tile: bool,
}

impl Placement {
    /// The placement a material layer holds.
    #[must_use]
    pub fn of(entity: &Entity) -> Self {
        let number =
            |key: &str, or: f64| entity.property(key).and_then(Value::as_f64).unwrap_or(or);
        Self {
            at: [
                number(MATERIAL_X, 0.0) as f32,
                number(MATERIAL_Y, 0.0) as f32,
            ],
            scale: (number(MATERIAL_SCALE, 1.0) as f32).max(1e-3),
            angle: number(MATERIAL_ANGLE, 0.0) as f32,
            tile: entity
                .property(MATERIAL_TILE)
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    /// Where on an image of `size` picture point `(px, py)` looks.
    #[must_use]
    pub fn look(&self, size: [u32; 2], px: f32, py: f32) -> [f32; 2] {
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let (dx, dy) = (
            (px - self.at[0]) / self.scale,
            (py - self.at[1]) / self.scale,
        );
        // Turned back by the angle.
        let (u, v) = (dx * cos + dy * sin, -dx * sin + dy * cos);
        [u + size[0] as f32 / 2.0, v + size[1] as f32 / 2.0]
    }
}

/// The raster an image material layer shows.
#[must_use]
pub fn material_image(arena: &Arena, layer: EntityId) -> Option<&RasterTile2D> {
    let image = arena
        .relations()
        .targets(layer, MATERIAL_IMAGE)
        .into_iter()
        .next()?;
    super::raster(arena, image)
}

/// The edit that makes a fill layer of `color` where a new layer goes with
/// `current` picked (see [`super::landing`]), and its id.
#[must_use]
pub fn new_fill_layer(
    arena: &Arena,
    picture: EntityId,
    current: Option<EntityId>,
    name: &str,
    color: Color,
) -> (Edit, EntityId) {
    let layer = Entity::empty(name, FILL_LAYER).with_property(FILL_COLOR, color);
    let id = layer.id();
    (
        Edit::new(tr!("new", name = name)).then(landed(arena, picture, current, layer)),
        id,
    )
}

/// The edit that makes a gradient layer of `gradient` where a new layer
/// goes with `current` picked, and its id.
#[must_use]
pub fn new_gradient_layer(
    arena: &Arena,
    picture: EntityId,
    current: Option<EntityId>,
    name: &str,
    gradient: Gradient,
) -> (Edit, EntityId) {
    let color = |[r, g, b, a]: [f32; 4]| Color {
        rgb: [r, g, b],
        alpha: a,
        space: ColorSpace::Srgb,
    };
    let [[x0, y0], [x1, y1]] = gradient.line;
    let layer = Entity::empty(name, GRADIENT_LAYER)
        .with_property(GRADIENT_FROM_X, f64::from(x0))
        .with_property(GRADIENT_FROM_Y, f64::from(y0))
        .with_property(GRADIENT_TO_X, f64::from(x1))
        .with_property(GRADIENT_TO_Y, f64::from(y1))
        .with_property(GRADIENT_START, color(gradient.colors[0]))
        .with_property(GRADIENT_END, color(gradient.colors[1]))
        .with_property(GRADIENT_RADIAL, gradient.radial);
    let id = layer.id();
    (
        Edit::new(tr!("new", name = name)).then(landed(arena, picture, current, layer)),
        id,
    )
}

/// The edit that makes an image material layer showing `image` placed as
/// `placement`, where a new layer goes with `current` picked, and its id.
#[must_use]
pub fn new_material_layer(
    arena: &Arena,
    picture: EntityId,
    current: Option<EntityId>,
    name: &str,
    image: RasterTile2D,
    placement: Placement,
) -> (Edit, EntityId) {
    let layer = Entity::empty(name, MATERIAL_LAYER)
        .with_property(MATERIAL_X, f64::from(placement.at[0]))
        .with_property(MATERIAL_Y, f64::from(placement.at[1]))
        .with_property(MATERIAL_SCALE, f64::from(placement.scale))
        .with_property(MATERIAL_ANGLE, f64::from(placement.angle))
        .with_property(MATERIAL_TILE, placement.tile);
    let id = layer.id();
    let image = Entity::new(name, image).with_class(MATERIAL_IMAGE_CLASS);
    let image_id = image.id();
    (
        Edit::new(tr!("new", name = name))
            .then(landed(arena, picture, current, layer))
            .then(SpawnEntity::new(image).with_relation(Relation::new(
                id,
                MATERIAL_IMAGE,
                image_id,
            ))),
        id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite::{RenderOptions, render};
    use crate::doc::{
        NewPicture, create, delete_layers, duplicate, layers, new_folder, rasterize, set,
    };
    use sue_core::{PixelFormat, UndoStack};
    use sue_model::picture;

    fn shown(arena: &Arena, picture: EntityId, width: u32) -> Vec<[u8; 4]> {
        render(arena, picture, [0, 0, width, 1], RenderOptions::default())
            .as_chunks::<4>()
            .0
            .to_vec()
    }

    fn start(width: u32) -> (Arena, EntityId, UndoStack<Arena>, EntityId) {
        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        let (edit, picture) = create(&NewPicture {
            width,
            height: 1,
            paper: Some(Color::srgb8(255, 255, 255)),
            ..NewPicture::default()
        })
        .unwrap();
        history.apply(&mut arena, edit).unwrap();
        let first = layers(&arena, picture).last().unwrap().id;
        (arena, picture, history, first)
    }

    #[test]
    fn a_fill_layer_stays_one_color_with_a_mode_and_rasterizes() {
        let (mut arena, picture, mut history, first) = start(2);
        let (edit, fill) = new_fill_layer(
            &arena,
            picture,
            Some(first),
            "Fill",
            Color::srgb8(0, 0, 255),
        );
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(shown(&arena, picture, 2), [[0, 0, 255, 255]; 2]);
        history
            .apply(&mut arena, set(fill, picture::BLEND, "multiply"))
            .unwrap();
        history
            .apply(&mut arena, set(fill, FILL_COLOR, Color::srgb8(255, 128, 0)))
            .unwrap();
        assert_eq!(shown(&arena, picture, 2), [[255, 128, 0, 255]; 2]);
        let before = shown(&arena, picture, 2);
        let (edit, flat) = rasterize(&arena, picture, fill).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(shown(&arena, picture, 2), before);
        assert_eq!(
            arena.get_by_id(flat).unwrap().property(picture::BLEND),
            Some(&Value::from("multiply")),
            "its mode stays a mode"
        );
        history.undo(&mut arena).unwrap();
        assert!(arena.get_by_id(fill).is_ok());
    }

    #[test]
    fn a_gradient_layer_runs_from_its_start_to_its_end() {
        let (mut arena, picture, mut history, first) = start(5);
        let gradient = Gradient {
            line: [[0.5, 0.0], [4.5, 0.0]],
            colors: [[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]],
            radial: false,
        };
        let (edit, layer) = new_gradient_layer(&arena, picture, Some(first), "Grad", gradient);
        history.apply(&mut arena, edit).unwrap();
        let row = shown(&arena, picture, 5);
        assert_eq!(row[0], [0, 0, 0, 255]);
        assert_eq!(row[2], [128, 128, 128, 255], "halfway, half gray");
        assert_eq!(row[4], [255, 255, 255, 255]);
        // Changed later: it follows.
        history
            .apply(&mut arena, set(layer, GRADIENT_TO_X, 2.5))
            .unwrap();
        assert_eq!(shown(&arena, picture, 5)[2], [255, 255, 255, 255]);
        assert_eq!(
            Gradient::of(arena.get_by_id(layer).unwrap()).line[1],
            [2.5, 0.0]
        );
    }

    #[test]
    fn an_image_material_is_placed_scaled_turned_and_tiled() {
        let (mut arena, picture, mut history, first) = start(8);
        // Two pixels: red then blue.
        let image = RasterTile2D::from_bytes(
            2,
            1,
            PixelFormat::Rgba8,
            [[255, 0, 0, 255], [0, 0, 255, 255]].concat(),
        )
        .unwrap();
        let place = Placement {
            at: [2.0, 0.5],
            scale: 1.0,
            angle: 0.0,
            tile: false,
        };
        let (edit, layer) =
            new_material_layer(&arena, picture, Some(first), "Pattern", image, place);
        history.apply(&mut arena, edit).unwrap();
        let row = shown(&arena, picture, 8);
        assert_eq!(row[1], [255, 0, 0, 255]);
        assert_eq!(row[2], [0, 0, 255, 255]);
        assert_eq!(row[5], [255, 255, 255, 255], "off the image, the paper");
        // Tiled, it repeats across.
        history
            .apply(&mut arena, set(layer, MATERIAL_TILE, true))
            .unwrap();
        let row = shown(&arena, picture, 8);
        assert_eq!(row[5], [255, 0, 0, 255]);
        assert_eq!(row[6], [0, 0, 255, 255]);
        // Twice as large, from its middle, smoothed between its pixels: a
        // quarter of the way from red to blue, then three quarters.
        history
            .apply(&mut arena, set(layer, MATERIAL_TILE, false))
            .unwrap();
        history
            .apply(&mut arena, set(layer, MATERIAL_SCALE, 2.0))
            .unwrap();
        let row = shown(&arena, picture, 8);
        assert_eq!(row[1], [191, 0, 64, 255]);
        assert_eq!(row[2], [64, 0, 191, 255]);
        // Half a turn: blue first.
        history
            .apply(&mut arena, set(layer, MATERIAL_SCALE, 1.0))
            .unwrap();
        history
            .apply(&mut arena, set(layer, MATERIAL_ANGLE, 180.0))
            .unwrap();
        let row = shown(&arena, picture, 8);
        assert_eq!(row[1], [0, 0, 255, 255]);
        assert_eq!(row[2], [255, 0, 0, 255]);
        // A copy has an image of its own; deleting takes the image along.
        let image_of = |arena: &Arena, layer| {
            arena
                .relations()
                .targets(layer, MATERIAL_IMAGE)
                .into_iter()
                .next()
        };
        let (edit, copy) = duplicate(&arena, picture, layer).unwrap();
        history.apply(&mut arena, edit).unwrap();
        let (own, theirs) = (image_of(&arena, layer), image_of(&arena, copy));
        assert!(theirs.is_some() && own != theirs);
        let edit = delete_layers(&arena, picture, &[copy]).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert!(
            arena.get_by_id(theirs.unwrap()).is_err(),
            "no image left over"
        );
        history.undo(&mut arena).unwrap();
        assert!(arena.get_by_id(theirs.unwrap()).is_ok());
        // In a folder, rasterized, it looks the same.
        let (edit, folder) = new_folder(&arena, picture, Some(layer), true);
        history.apply(&mut arena, edit).unwrap();
        let before = shown(&arena, picture, 8);
        let (edit, _) = rasterize(&arena, picture, layer).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(shown(&arena, picture, 8), before);
        assert!(
            arena.get_by_id(own.unwrap()).is_err(),
            "its image went with it"
        );
        assert!(crate::doc::members(&arena, picture, folder).len() == 1);
    }
}
